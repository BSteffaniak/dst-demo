#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::time::Duration;

use dst_demo_server_simulator::{client, handle_actions, host};
use dst_demo_simulator_harness::{
    SIMULATOR_CANCELLATION_TOKEN,
    random::RNG,
    run_simulation,
    time::simulator::STEP_MULTIPLIER,
    turmoil::{self},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    unsafe {
        dst_demo_simulator_harness::init();
    }

    pretty_env_logger::init();

    ctrlc::set_handler(move || SIMULATOR_CANCELLATION_TOKEN.cancel())
        .expect("Error setting Ctrl-C handler");

    let duration_secs = std::env::var("SIMULATOR_DURATION")
        .ok()
        .map_or(u64::MAX, |x| x.parse::<u64>().unwrap());

    let mut sim = turmoil::Builder::new()
        .simulation_duration(Duration::MAX)
        .tick_duration(Duration::from_millis(*STEP_MULTIPLIER))
        .build_with_rng(Box::new(RNG.clone()));

    host::server::start(&mut sim);

    client::health_checker::start(&mut sim);
    client::fault_injector::start(&mut sim);
    client::banker::start(&mut sim);

    run_simulation(&mut sim, duration_secs, |sim| {
        handle_actions(sim);
    })
    .unwrap()
}
