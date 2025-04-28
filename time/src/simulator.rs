use std::{
    cell::RefCell,
    sync::RwLock,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

thread_local! {
    static EPOCH_OFFSET: RefCell<RwLock<Option<u64>>> = const { RefCell::new(RwLock::new(None)) };
}

fn gen_epoch_offset() -> u64 {
    let value = dst_demo_random::rng().gen_range(1..100_000_000_000_000u64);

    std::env::var("SIMULATOR_EPOCH_OFFSET")
        .ok()
        .map_or(value, |x| x.parse::<u64>().unwrap())
}

/// # Panics
///
/// * If the `EPOCH_OFFSET` `RwLock` fails to write to
pub fn reset_epoch_offset() {
    let value = gen_epoch_offset();
    log::debug!("reset_epoch_offset to seed={value}");
    EPOCH_OFFSET.with_borrow_mut(|x| *x.write().unwrap() = Some(value));
}

/// # Panics
///
/// * If the `EPOCH_OFFSET` `RwLock` fails to read from
#[must_use]
pub fn epoch_offset() -> u64 {
    let value = EPOCH_OFFSET.with_borrow(|x| *x.read().unwrap());
    value.unwrap_or_else(|| {
        let value = gen_epoch_offset();
        EPOCH_OFFSET.with_borrow_mut(|x| *x.write().unwrap() = Some(value));
        value
    })
}

thread_local! {
    static STEP_MULTIPLIER: RefCell<RwLock<Option<u64>>> = const { RefCell::new(RwLock::new(None)) };
}

fn gen_step_multiplier() -> u64 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let value = {
        let value = dst_demo_random::rng().gen_range_disti(1..1_000_000, 10);
        if value == 0 { 1 } else { value }
    };
    std::env::var("SIMULATOR_STEP_MULTIPLIER")
        .ok()
        .map_or(value, |x| x.parse::<u64>().unwrap())
}

/// # Panics
///
/// * If the `STEP_MULTIPLIER` `RwLock` fails to write to
pub fn reset_step_multiplier() {
    let value = gen_step_multiplier();
    log::debug!("reset_step_multiplier to seed={value}");
    STEP_MULTIPLIER.with_borrow_mut(|x| *x.write().unwrap() = Some(value));
}

/// # Panics
///
/// * If the `STEP_MULTIPLIER` `RwLock` fails to read from
#[must_use]
pub fn step_multiplier() -> u64 {
    let value = STEP_MULTIPLIER.with_borrow(|x| *x.read().unwrap());
    value.unwrap_or_else(|| {
        let value = gen_epoch_offset();
        STEP_MULTIPLIER.with_borrow_mut(|x| *x.write().unwrap() = Some(value));
        value
    })
}

/// # Panics
///
/// * If the simulated `UNIX_EPOCH` offset is larger than a `u64` can store
#[must_use]
pub fn now() -> SystemTime {
    let epoch_offset = epoch_offset();
    let step_multiplier = step_multiplier();
    let step = dst_demo_simulator_utils::current_step();
    let mult_step = step.checked_mul(step_multiplier).unwrap();
    let millis = epoch_offset.checked_add(mult_step).unwrap();
    log::debug!(
        "now: epoch_offset={epoch_offset} step={step} step_multiplier={step_multiplier} millis={millis}"
    );
    UNIX_EPOCH
        .checked_add(Duration::from_millis(millis))
        .unwrap()
}
