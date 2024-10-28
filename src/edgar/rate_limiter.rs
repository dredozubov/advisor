use std::sync::Arc;
use tokio::sync::Semaphore;

pub struct RateLimiter {
    semaphore: Arc<Semaphore>,
}

impl RateLimiter {
    pub fn new(max_concurrent: usize) -> Self {
        RateLimiter {
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
        }
    }

    pub async fn acquire(&self) -> tokio::sync::SemaphorePermit {
        self.semaphore.acquire().await.expect("Semaphore closed")
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new(10) // SEC allows 10 requests per second
    }
}
