#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::time::Duration;

use dst_demo_server_simulator::{
    SIMULATOR_CANCELLATION_TOKEN, client, formatting::TimeFormat as _, handle_actions, host,
};
use dst_demo_simulator_harness::{
    random::RNG,
    time::simulator::{EPOCH_OFFSET, STEP_MULTIPLIER},
    turmoil::{self},
    utils::{SEED, STEP},
};

fn run_info() -> String {
    format!(
        "\
        seed={seed}\n\
        epoch_offset={epoch_offset}\n\
        step_multiplier={step_multiplier}",
        seed = *SEED,
        epoch_offset = *EPOCH_OFFSET,
        step_multiplier = *STEP_MULTIPLIER,
    )
}

#[allow(clippy::cast_precision_loss)]
fn run_info_end(
    successful: bool,
    real_time_millis: u128,
    system_time_millis: u128,
    step: u32,
) -> String {
    format!(
        "\
        {run_info}\n\
        successful={successful}\n\
        real_time_elapsed={real_time}\n\
        simulated_system_time_elapsed={simulated_system_time} ({simulated_system_time_x:.2}x)\n\
        simulated_time_elapsed={simulated_time} ({simulated_time_x:.2}x)",
        run_info = run_info(),
        real_time = real_time_millis.into_formatted(),
        simulated_system_time = system_time_millis.into_formatted(),
        simulated_system_time_x = system_time_millis as f64 / real_time_millis as f64,
        simulated_time = step.into_formatted(),
        simulated_time_x = f64::from(step) / real_time_millis as f64,
    )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    unsafe {
        dst_demo_simulator_harness::init();
    }

    pretty_env_logger::init();

    log::info!("Server simulator starting\n{}", run_info());

    ctrlc::set_handler(move || SIMULATOR_CANCELLATION_TOKEN.cancel())
        .expect("Error setting Ctrl-C handler");

    let duration_secs = std::env::var("SIMULATOR_DURATION")
        .ok()
        .map_or(u64::MAX, |x| x.parse::<u64>().unwrap());

    let start_system = dst_demo_simulator_harness::time::now();
    let start = std::time::SystemTime::now();
    STEP.store(1, std::sync::atomic::Ordering::SeqCst);

    let resp = std::panic::catch_unwind(|| run_simulation(duration_secs));
    let step = STEP.load(std::sync::atomic::Ordering::SeqCst);

    let end_system = dst_demo_simulator_harness::time::now();
    let system_time_millis = end_system.duration_since(start_system).unwrap().as_millis();
    let end = std::time::SystemTime::now();
    let real_time_millis = end.duration_since(start).unwrap().as_millis();

    log::info!(
        "Server simulator finished\n{}",
        run_info_end(
            resp.as_ref().is_ok_and(Result::is_ok),
            real_time_millis,
            system_time_millis,
            step,
        )
    );

    resp.unwrap()
}

fn run_simulation(duration_secs: u64) -> Result<(), Box<dyn std::error::Error>> {
    let mut sim = turmoil::Builder::new()
        .simulation_duration(Duration::from_secs(duration_secs))
        .build_with_rng(Box::new(RNG.clone()));

    host::server::start(&mut sim);

    client::health_checker::start(&mut sim);
    client::fault_injector::start(&mut sim);
    client::banker::start(&mut sim);

    while !SIMULATOR_CANCELLATION_TOKEN.is_cancelled() {
        let step = STEP.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        if step % 1000 == 0 {
            #[allow(clippy::cast_precision_loss)]
            if duration_secs < u64::MAX {
                log::info!(
                    "step {step} ({:.1}%)",
                    (f64::from(step) / duration_secs as f64 / 10.0)
                );
            } else {
                log::info!("step {step}");
            }
        }

        handle_actions(&mut sim);

        match sim.step() {
            Ok(..) => {}
            Err(e) => {
                let message = e.to_string();
                if message.starts_with("Ran for duration: ")
                    && message.ends_with(" without completing")
                {
                    break;
                }
                return Err(e);
            }
        }
    }

    if !SIMULATOR_CANCELLATION_TOKEN.is_cancelled() {
        SIMULATOR_CANCELLATION_TOKEN.cancel();
    }

    Ok(())
}
