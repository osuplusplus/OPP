//! 有限的重任务线程预算。异步运行时只负责等待，计算在独立线程池中执行。
//!
//! 短交互与批处理隔离；嵌套 par_iter 自动复用当前池，禁止再建全核线程池。
//! 音频、渲染及进程监控等长期循环不应占用这些线程。
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, OnceLock},
    time::Instant,
};

use rayon::{ThreadPool, ThreadPoolBuilder};
use tokio::sync::{Semaphore, oneshot};

use crate::error::{CommandError, CommandResult};

pub(crate) struct WorkerPool {
    pool: ThreadPool,
    slots: Arc<Semaphore>,
}

impl WorkerPool {
    fn new(name: &'static str, threads: usize) -> CommandResult<Self> {
        let pool = ThreadPoolBuilder::new()
            .num_threads(threads)
            .thread_name(move |index| format!("opp-{name}-{index}"))
            .build()
            .map_err(|error| CommandError::new("TASK_POOL_INIT_FAILED", error.to_string()))?;
        Ok(Self {
            pool,
            slots: Arc::new(Semaphore::new(128)),
        })
    }

    /// 仅供同步后台线程使用；调用方会等待，不能从 UI 或 async 上下文调用。
    pub(crate) fn install<F, T>(&self, work: F) -> T
    where
        F: FnOnce() -> T + Send,
        T: Send,
    {
        self.pool.install(work)
    }

    async fn run<F, T>(&self, operation: &'static str, work: F) -> CommandResult<T>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        // 限制已提交任务，避免快速滚动等请求洪峰无限积压闭包及资源。
        let permit = self
            .slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| CommandError::new("TASK_QUEUE_FULL", "后台任务过多，请稍后重试"))?;
        let queued = Instant::now();
        let (tx, rx) = oneshot::channel();
        self.pool.spawn_fifo(move || {
            let _permit = permit;
            // 仅取消尚未开始的任务；已开始的写入仍须完成，扫描使用自己的取消标志。
            if tx.is_closed() {
                return;
            }
            let mut span =
                super::logging::global().map(|logger| logger.operation("tasks", operation));
            if let Some(span) = &span {
                span.info(
                    "重任务开始执行",
                    Some(serde_json::json!({
                        "queue_ms": queued.elapsed().as_millis() as u64,
                        "thread": std::thread::current().name(),
                    })),
                );
            }
            let result = catch_unwind(AssertUnwindSafe(work))
                .map_err(|_| CommandError::new("TASK_PANICKED", "后台任务异常结束"));
            if let Some(span) = &mut span {
                match &result {
                    Ok(_) => span.finish_ok(None),
                    Err(error) => span.finish_error(error),
                }
            }
            drop(_permit);
            let _ = tx.send(result);
        });
        rx.await
            .map_err(|_| CommandError::new("TASK_STOPPED", "后台任务已停止"))?
    }
}

// 为交互保留一个线程，后台最多使用三个线程；低配机器不再固定开四个扫描线程。
fn background_threads(cores: usize) -> usize {
    cores.saturating_sub(2).clamp(1, 3)
}

pub(crate) fn background_pool() -> CommandResult<&'static WorkerPool> {
    static POOL: OnceLock<CommandResult<WorkerPool>> = OnceLock::new();
    POOL.get_or_init(|| {
        let cores = std::thread::available_parallelism().map_or(1, usize::from);
        WorkerPool::new("background", background_threads(cores))
    })
    .as_ref()
    .map_err(Clone::clone)
}

pub(crate) async fn background<F, T>(operation: &'static str, work: F) -> CommandResult<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    background_pool()?.run(operation, work).await
}

pub(crate) async fn interactive<F, T>(operation: &'static str, work: F) -> CommandResult<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    static POOL: OnceLock<CommandResult<WorkerPool>> = OnceLock::new();
    POOL.get_or_init(|| WorkerPool::new("interactive", 1))
        .as_ref()
        .map_err(Clone::clone)?
        .run(operation, work)
        .await
}

/// 文件系统和外部进程等待独立于计算线程，最多同时两个操作。
pub(crate) async fn blocking_io<F, T>(operation: &'static str, work: F) -> CommandResult<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    static POOL: OnceLock<CommandResult<WorkerPool>> = OnceLock::new();
    POOL.get_or_init(|| WorkerPool::new("blocking-io", 2))
        .as_ref()
        .map_err(Clone::clone)?
        .run(operation, work)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaves_capacity_for_interaction_and_caps_large_machines() {
        assert_eq!([1, 2, 4, 8, 64].map(background_threads), [1, 1, 2, 3, 3]);
    }

    #[tokio::test]
    async fn nested_parallel_work_uses_the_budget_and_panics_do_not_kill_pool() {
        use rayon::prelude::*;
        let pool = WorkerPool::new("test", 1).unwrap();
        let count = pool
            .run("parallel", || {
                (0..20)
                    .into_par_iter()
                    .map(|_| rayon::current_num_threads())
                    .max()
            })
            .await
            .unwrap();
        assert_eq!(count, Some(1));
        assert_eq!(
            pool.run("panic", || panic!("test")).await.unwrap_err().code,
            "TASK_PANICKED"
        );
        assert_eq!(pool.run("after_panic", || 42).await.unwrap(), 42);
    }

    #[tokio::test]
    async fn dropping_queued_work_skips_it_and_releases_capacity() {
        let pool = WorkerPool::new("test", 1).unwrap();
        let (started_tx, started_rx) = oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        pool.pool.spawn(move || {
            started_tx.send(()).unwrap();
            release_rx.recv().unwrap();
        });
        started_rx.await.unwrap();
        let ran = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let marker = ran.clone();
        let mut pending = Box::pin(pool.run("cancelled", move || {
            marker.store(true, std::sync::atomic::Ordering::SeqCst);
        }));
        assert!(futures_util::poll!(&mut pending).is_pending());
        drop(pending);
        release_tx.send(()).unwrap();
        pool.run("barrier", || ()).await.unwrap();
        assert!(!ran.load(std::sync::atomic::Ordering::SeqCst));
        assert_eq!(pool.slots.available_permits(), 128);
    }

    #[tokio::test]
    async fn saturated_background_does_not_block_interactive_or_async_runtime() {
        let background = WorkerPool::new("test-background", 1).unwrap();
        let interactive = WorkerPool::new("test-interactive", 1).unwrap();
        let (started_tx, started_rx) = oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        background.pool.spawn(move || {
            started_tx.send(()).unwrap();
            let _ = release_rx.recv();
        });
        started_rx.await.unwrap();
        let value = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            interactive.run("query", || 7),
        )
        .await
        .expect("interactive lane must stay responsive")
        .unwrap();
        assert_eq!(value, 7);
        release_tx.send(()).unwrap();
    }

    #[tokio::test]
    async fn overload_is_rejected_without_starting_more_work() {
        let pool = WorkerPool::new("test", 1).unwrap();
        let permits = pool.slots.clone().acquire_many_owned(128).await.unwrap();
        let error = pool
            .run("overflow", || panic!("must not execute"))
            .await
            .unwrap_err();
        assert_eq!(error.code, "TASK_QUEUE_FULL");
        drop(permits);
        assert_eq!(pool.run("recovered", || 1).await.unwrap(), 1);
    }
}
