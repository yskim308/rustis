use std::sync::{
    atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    Arc,
};

use tokio::sync::Notify;

#[derive(Debug)]
pub struct SleepState {
    notify: Arc<Notify>,
    is_asleep: AtomicBool,
}

impl SleepState {
    pub fn new() -> Self {
        Self {
            notify: Arc::new(Notify::new()),
            is_asleep: AtomicBool::new(true), // Start asleep
        }
    }
    /// Notify consumer if sleeping, returns true if notification was sent
    pub fn notify_if_asleep(&self) -> bool {
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
    pub async fn enter_sleep(&self, config: &CooldownConfig) {
        self.is_asleep.store(true, Ordering::Release);

        tokio::select! {
            _ = self.notify.notified() => {
                // Woken up by notification
                self.pending_notifications.fetch_sub(1, Ordering::Relaxed);
            }
            _ = tokio::time::sleep(config.active_poll_duration) => {
                // Cooldown period ended naturally
            }
        }
    }
}
