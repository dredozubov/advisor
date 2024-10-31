use std::sync::Arc;
use tokio::sync::Semaphore;

#[derive(Clone)]
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
