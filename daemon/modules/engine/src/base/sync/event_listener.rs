use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::Semaphore;

pub struct EventListener {
    semaphore: Semaphore,
    notified: AtomicBool,
}

impl EventListener {
    pub fn new() -> Self {
        Self {
            semaphore: Semaphore::new(0),
            notified: AtomicBool::new(false),
        }
    }

    pub fn notify(&self) {
        if self.notified.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
            self.semaphore.add_permits(1);
        }
    }

    pub async fn wait(&self) {
        self.notified.store(false, Ordering::SeqCst);
        let _permit = self.semaphore.acquire().await;
    }
}

impl Default for EventListener {
    fn default() -> Self {
        Self::new()
    }
}
