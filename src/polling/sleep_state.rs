use std::sync::{
    atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    Arc,
};

use tokio::sync::Notify;

#[derive(Debug, Default)]
pub struct SleepState {
    notify: Arc<Notify>,
    is_asleep: AtomicBool,
}

impl SleepState {
    pub fn new() -> Self {
        Self::default()
    }
    /// Notify consumer if sleeping, returns true if notification was sent
    pub fn notify_if_asleep(&self) -> bool {
        if !self.is_asleep.load(Ordering::Relaxed) {
            return false;
        }

        let was_asleep = self
            .is_asleep
            .compare_exchange(
                true,
                false, // Expected: sleeping, New: awake
                Ordering::AcqRel,
                Ordering::Relaxed,
            )
            .unwrap_or(false);

        if was_asleep {
            self.notify.notify_one();
        }
        was_asleep
    }

    /// Mark as sleeping and wait for notification or timeout
    pub async fn enter_sleep(&self) {
        self.is_asleep.store(true, Ordering::Release);

        self.notify.notified().await;
    }
}
