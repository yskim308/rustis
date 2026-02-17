use futures::task::AtomicWaker;
use std::sync::atomic::{AtomicBool, Ordering};

#[repr(align(64))]
#[derive(Debug)]
pub struct TaskNotifier {
    pub waker: AtomicWaker,
    pub is_sleeping: AtomicBool,
}

impl TaskNotifier {
    pub fn new() -> Self {
        Self {
            waker: AtomicWaker::new(),
            is_sleeping: AtomicBool::new(false),
        }
    }

    // Helper to make the call-site clean
    pub fn wake_if_sleeping(&self) {
        if self
            .is_sleeping
            .compare_exchange(true, false, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            self.waker.wake();
        }
    }
}
