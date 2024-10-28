use once_cell::sync::Lazy;
use std::sync::Arc;
use tokio::sync::Semaphore;

static GLOBAL_RATE_LIMITER: Lazy<RateLimiter> = Lazy::new(|| RateLimiter::new(10));

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

    pub fn global() -> &'static RateLimiter {
        &GLOBAL_RATE_LIMITER
    }
}
