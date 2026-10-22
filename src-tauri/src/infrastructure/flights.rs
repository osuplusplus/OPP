//! Share only concurrent loads; completed values are owned by existing cache layers.
//! 单飞（singleflight）去重：同一键的并发加载只执行一次；结果（含错误）仅
//! 在飞行期间共享，完成后不再驻留，缓存语义完全交由上层缓存层负责。
use std::{
    collections::HashMap,
    hash::Hash,
    sync::{Arc, Mutex, Weak},
};
use tokio::sync::OnceCell;

/// 键到“飞行中”加载槽的注册表。槽以 `Weak` 持有：最后一个等待者离开后
/// 注册项即可被清除，已完成的值不会在这里长期驻留。
pub(crate) struct Flights<K, V>(Mutex<HashMap<K, Weak<OnceCell<V>>>>);
impl<K, V> Default for Flights<K, V> {
    fn default() -> Self {
        Self(Mutex::new(HashMap::new()))
    }
}
impl<K: Eq + Hash, V: Clone> Flights<K, V> {
    /// 执行（或加入）键对应的加载：并发调用共享同一次 `load` 的结果，
    /// 包括错误；全部等待者拿到结果后，下一次调用会重新执行加载。
    pub(crate) async fn run<F: std::future::Future<Output = V>>(
        &self,
        key: K,
        load: impl FnOnce() -> F,
    ) -> V {
        let slot = {
            let mut flights = self.0.lock().unwrap_or_else(|error| error.into_inner());
            flights.retain(|_, slot| slot.strong_count() > 0);
            if let Some(slot) = flights.get(&key).and_then(Weak::upgrade) {
                slot
            } else {
                let slot = Arc::new(OnceCell::new());
                flights.insert(key, Arc::downgrade(&slot));
                slot
            }
        };
        slot.get_or_init(load).await.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    // 飞行中的错误应被并发方共享，但之后的调用允许重试并成功。
    #[tokio::test]
    async fn shares_in_flight_errors_but_retries_later() {
        let flights = Flights::<u8, Result<u8, ()>>::default();
        let calls = AtomicUsize::new(0);
        let load = || async {
            calls.fetch_add(1, Ordering::SeqCst);
            tokio::task::yield_now().await;
            Err(())
        };
        let (a, b) = tokio::join!(flights.run(1, load), flights.run(1, load));
        assert!(a.is_err() && b.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(flights.run(1, || async { Ok(7) }).await, Ok(7));
    }
}
