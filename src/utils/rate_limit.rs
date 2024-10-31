use once_cell::sync::OnceCell;
use std::sync::Arc;
use tokio::sync::Semaphore;

pub struct RateLimiter {
    semaphore: Arc<Semaphore>,
}

static EDGAR_RATE_LIMITER: OnceCell<RateLimiter> = OnceCell::new();

impl RateLimiter {
    pub fn new(max_concurrent: usize) -> Self {
        RateLimiter {
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
        }
    }

    pub async fn acquire(&self) -> tokio::sync::SemaphorePermit {
        self.semaphore.acquire().await.expect("Semaphore closed")
    }

    pub fn edgar() -> &'static RateLimiter {
        EDGAR_RATE_LIMITER.get_or_init(|| RateLimiter::new(10))
    }
}
