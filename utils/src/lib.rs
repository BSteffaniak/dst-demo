#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::sync::LazyLock;

pub static SEED: LazyLock<u64> = LazyLock::new(|| {
    std::env::var("SIMULATOR_SEED")
        .ok()
        .and_then(|x| x.parse::<u64>().ok())
        .unwrap_or_else(|| getrandom::u64().unwrap())
});

#[must_use]
pub fn simulator_enabled() -> bool {
    std::env::var("ENABLE_SIMULATOR").as_deref() == Ok("1")
}
