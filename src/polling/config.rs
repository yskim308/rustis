use tokio::time::Duration;

#[derive(Debug, Clone)]
pub struct CooldownConfig {
    /// Continue polling for this duration after wakeup (default: 250μs)
    pub active_poll_duration: Duration,
    /// Enter sleep after this many consecutive empty polls (default: 100)
    pub max_idle_cycles: u32,
    /// Maximum pending notifications before dropping (default: 1000)
    pub max_pending_notifications: u32,
}
impl Default for CooldownConfig {
    fn default() -> Self {
        Self {
            active_poll_duration: Duration::from_micros(250),
            max_idle_cycles: 100,
            max_pending_notifications: 1000,
        }
    }
}
