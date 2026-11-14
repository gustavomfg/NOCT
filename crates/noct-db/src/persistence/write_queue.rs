//! Serializes the writes that share a key, such as the files of one workspace, without changing
//! their results. Port of `electron/persistence/SerializedWriteQueue.ts`.
//!
//! Operations on different keys run in parallel. A failed operation does not block the ones
//! after it, and a key that has no pending operation is forgotten.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

#[derive(Default)]
pub struct SerializedWrites {
    queues: Mutex<HashMap<String, Arc<Mutex<()>>>>,
}

impl SerializedWrites {
    pub fn new() -> Self {
        Self::default()
    }

    /// Runs `operation` once every earlier operation on `key` has finished, and returns its
    /// result. A panic in `operation` does not stop later operations on the same key.
    pub fn run<T>(&self, key: &str, operation: impl FnOnce() -> T) -> T {
        // Declared first, so it runs last: after this call has released its own handle to the
        // queue, on a normal return and on a panic alike.
        let _forget = Forget { writes: self, key };
        let queue = lock(&self.queues)
            .entry(key.to_owned())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone();
        let _turn = lock(&queue);
        operation()
    }

    /// Removes the queue of `key` when no operation holds or waits for it. The map lock is held
    /// while checking, so an operation that starts later finds the queue or recreates it.
    fn forget_if_idle(&self, key: &str) {
        let mut queues = lock(&self.queues);
        if queues
            .get(key)
            .is_some_and(|queue| Arc::strong_count(queue) == 1)
        {
            queues.remove(key);
        }
    }
}

/// Forgets an idle key when the operation that used it ends.
struct Forget<'a> {
    writes: &'a SerializedWrites,
    key: &'a str,
}

impl Drop for Forget<'_> {
    fn drop(&mut self) {
        self.writes.forget_if_idle(self.key);
    }
}

/// A poisoned lock still guards consistent data here, because a panic never leaves a queue
/// half-updated, so the queue continues rather than propagating the poison.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::{SerializedWrites, lock};

    #[test]
    fn idle_keys_are_forgotten() {
        let writes = SerializedWrites::new();
        writes.run("workspace-a", || ());
        writes.run("workspace-b", || ());

        assert_eq!(lock(&writes.queues).len(), 0);
    }

    #[test]
    fn a_panicked_operation_leaves_no_key_behind() {
        let writes = SerializedWrites::new();

        let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            writes.run("workspace-a", || panic!("falha"))
        }));

        assert!(failed.is_err());
        assert_eq!(lock(&writes.queues).len(), 0);
    }
}
