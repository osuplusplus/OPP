//! Fallible, retryable initialization on the calling worker, before any access.
use crate::error::{CommandError, CommandResult};
use std::{
    ops::{Deref, DerefMut},
    sync::{Mutex, MutexGuard},
};

/// 可失败、可重试的惰性初始化互斥锁：首次加锁时在调用线程上完成初始化。
///
/// 与 `OnceLock` 不同，失败的初始化不会驻留，下次加锁会重新尝试。
pub(crate) struct LazyMutex<T> {
    value: Mutex<Option<T>>,
    initialize: Box<dyn Fn() -> CommandResult<T> + Send + Sync>,
}

impl<T> LazyMutex<T> {
    /// 包装初始化闭包；闭包须容忍失败后被再次调用。
    pub(crate) fn new(initialize: impl Fn() -> CommandResult<T> + Send + Sync + 'static) -> Self {
        Self {
            value: Mutex::new(None),
            initialize: Box::new(initialize),
        }
    }
    /// 加锁并按需初始化：初始化在持锁状态下执行，其余线程阻塞等待而非并发重建。
    /// 内部锁一旦中毒（持守卫期间 panic）即永久失效，之后一律返回 `STATE_ERROR`。
    pub(crate) fn lock(&self) -> CommandResult<LazyGuard<'_, T>> {
        let mut value = self
            .value
            .lock()
            .map_err(|_| CommandError::new("STATE_ERROR", "状态锁已损坏"))?;
        if value.is_none() {
            *value = Some((self.initialize)()?);
        }
        Ok(LazyGuard(value))
    }
}
/// 初始化完成后的守卫：对外表现为 `&T`/`&mut T`，生命周期与内部锁绑定。
pub(crate) struct LazyGuard<'a, T>(MutexGuard<'a, Option<T>>);
impl<T> Deref for LazyGuard<'_, T> {
    type Target = T;
    // 守卫只能经 lock() 构造，发布前必已写入 Some，expect 不会实际触发。
    fn deref(&self) -> &T {
        self.0.as_ref().expect("initialized before publication")
    }
}
impl<T> DerefMut for LazyGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.0.as_mut().expect("initialized before publication")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    // 验证初始化失败可重试、成功后只执行一次，且守卫支持可变访问。
    #[test]
    fn retries_failure_and_initializes_once_before_mutation() {
        let calls = Arc::new(AtomicUsize::new(0));
        let count = calls.clone();
        let value = LazyMutex::new(move || {
            if count.fetch_add(1, Ordering::SeqCst) == 0 {
                return Err(CommandError::new("TEST", "retry"));
            }
            Ok(vec![1])
        });
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert!(value.lock().is_err());
        value.lock().unwrap().push(2);
        assert_eq!(&*value.lock().unwrap(), &[1, 2]);
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }
}
