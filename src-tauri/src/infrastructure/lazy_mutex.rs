//! Fallible, retryable initialization on the calling worker, before any access.
use crate::error::{CommandError, CommandResult};
use std::{
    ops::{Deref, DerefMut},
    sync::{Mutex, MutexGuard},
};

pub(crate) struct LazyMutex<T> {
    value: Mutex<Option<T>>,
    initialize: Box<dyn Fn() -> CommandResult<T> + Send + Sync>,
}

impl<T> LazyMutex<T> {
    pub(crate) fn new(initialize: impl Fn() -> CommandResult<T> + Send + Sync + 'static) -> Self {
        Self {
            value: Mutex::new(None),
            initialize: Box::new(initialize),
        }
    }
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
pub(crate) struct LazyGuard<'a, T>(MutexGuard<'a, Option<T>>);
impl<T> Deref for LazyGuard<'_, T> {
    type Target = T;
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
