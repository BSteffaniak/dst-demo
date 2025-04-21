use ::std::time::{Duration, SystemTime, UNIX_EPOCH};

#[must_use]
pub fn now() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(dst_demo_random::RNG.next_u64())
}
