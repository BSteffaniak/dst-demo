use std::{
    sync::{LazyLock, RwLock},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use dst_demo_random::non_uniform_distribute_i32;

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
static EPOCH_OFFSET: LazyLock<RwLock<u64>> = LazyLock::new(|| RwLock::new(gen_epoch_offset()));

static STEP_MULTIPLIER: LazyLock<RwLock<u64>> =
    LazyLock::new(|| RwLock::new(gen_step_multiplier()));

fn gen_epoch_offset() -> u64 {
    let value = dst_demo_random::RNG.gen_range(1..100_000_000_000_000u64);

    std::env::var("SIMULATOR_EPOCH_OFFSET")
        .ok()
        .map_or(value, |x| x.parse::<u64>().unwrap())
}

/// # Panics
///
/// * If the `EPOCH_OFFSET` `RwLock` fails to write to
pub fn reset_epoch_offset() -> u64 {
    let value = gen_epoch_offset();
    *EPOCH_OFFSET.write().unwrap() = value;
    value
}

/// # Panics
///
/// * If the `EPOCH_OFFSET` `RwLock` fails to read from
#[must_use]
pub fn epoch_offset() -> u64 {
    *EPOCH_OFFSET.read().unwrap()
}

fn gen_step_multiplier() -> u64 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let value = {
        let rng = &dst_demo_random::RNG;
        let value = rng.gen_range(1..1_000_000);
        let value = non_uniform_distribute_i32!(value, 10) as u64;
        if value == 0 { 1 } else { value }
    };
    std::env::var("SIMULATOR_STEP_MULTIPLIER")
        .ok()
        .map_or(value, |x| x.parse::<u64>().unwrap())
}

/// # Panics
///
/// * If the `STEP_MULTIPLIER` `RwLock` fails to write to
pub fn reset_step_multiplier() -> u64 {
    let value = gen_step_multiplier();
    *STEP_MULTIPLIER.write().unwrap() = value;
    value
}

/// # Panics
///
/// * If the `STEP_MULTIPLIER` `RwLock` fails to read from
#[must_use]
pub fn step_multiplier() -> u64 {
    *STEP_MULTIPLIER.read().unwrap()
}

/// # Panics
///
/// * If the simulated `UNIX_EPOCH` offset is larger than a `u64` can store
#[must_use]
pub fn now() -> SystemTime {
    let epoch_offset = epoch_offset();
    let step_multiplier = step_multiplier();
    let step = u64::from(dst_demo_simulator_utils::step());
    let mult_step = step.checked_mul(step_multiplier).unwrap();
    let millis = epoch_offset.checked_add(mult_step).unwrap();
    log::debug!(
        "now: epoch_offset={epoch_offset} step={step} step_multiplier={step_multiplier} millis={millis}"
    );
    UNIX_EPOCH
        .checked_add(Duration::from_millis(millis))
        .unwrap()
}
