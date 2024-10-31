use once_cell::sync::OnceCell;
use crate::utils::rate_limit::RateLimiter;

pub mod filing;
pub mod query;
pub mod report;
pub mod tickers;
pub mod xbrl;

static RATE_LIMITER: OnceCell<RateLimiter> = OnceCell::new();

pub(crate) fn rate_limiter() -> &'static RateLimiter {
    RATE_LIMITER.get_or_init(|| RateLimiter::new(10))
}
