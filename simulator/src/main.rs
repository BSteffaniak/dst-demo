#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    panic::AssertUnwindSafe,
    time::{Duration, SystemTime},
};

use dst_demo_server_simulator::{
    SIMULATOR_CANCELLATION_TOKEN, client, formatting::TimeFormat as _, handle_actions, host,
};
use dst_demo_simulator_harness::{
    random::RNG,
    time::simulator::{EPOCH_OFFSET, STEP_MULTIPLIER},
    turmoil::{self, Sim},
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
fn run_info_end(successful: bool, real_time_millis: u128, sim_time_millis: u128) -> String {
    format!(
        "\
        {run_info}\n\
        successful={successful}\n\
        real_time_elapsed={real_time}\n\
        simulated_time_elapsed={simulated_time} ({simulated_time_x:.2}x)",
        run_info = run_info(),
        real_time = real_time_millis.into_formatted(),
        simulated_time = sim_time_millis.into_formatted(),
        simulated_time_x = sim_time_millis as f64 / real_time_millis as f64,
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

    let start = SystemTime::now();
    STEP.store(1, std::sync::atomic::Ordering::SeqCst);

    let mut sim = turmoil::Builder::new()
        .simulation_duration(Duration::MAX)
        .tick_duration(duration_from_step_multiplier())
        .build_with_rng(Box::new(RNG.clone()));

    let resp = std::panic::catch_unwind(AssertUnwindSafe(|| {
        run_simulation(&mut sim, duration_secs, start)
    }));

    let end = SystemTime::now();
    let real_time_millis = end.duration_since(start).unwrap().as_millis();
    let sim_time_millis = sim.elapsed().as_millis();

    log::info!(
        "Server simulator finished\n{}",
        run_info_end(
            resp.as_ref().is_ok_and(Result::is_ok),
            real_time_millis,
            sim_time_millis,
        )
    );

    resp.unwrap()
}

fn duration_from_step_multiplier() -> Duration {
    Duration::from_millis(*STEP_MULTIPLIER)
}

fn run_simulation(
    sim: &mut Sim<'_>,
    duration_secs: u64,
    start: SystemTime,
) -> Result<(), Box<dyn std::error::Error>> {
    let print_step = |sim: &Sim<'_>, step| {
        #[allow(clippy::cast_precision_loss)]
        if duration_secs < u64::MAX {
            log::info!(
                "step {step} ({}) ({:.1}%)",
                sim.elapsed().as_millis().into_formatted(),
                SystemTime::now().duration_since(start).unwrap().as_millis() as f64
                    / (duration_secs as f64 * 1000.0)
                    * 100.0,
            );
        } else {
            log::info!(
                "step {step} ({})",
                sim.elapsed().as_millis().into_formatted()
            );
        }
    };

    host::server::start(sim);

    client::health_checker::start(sim);
    client::fault_injector::start(sim);
    client::banker::start(sim);

    while !SIMULATOR_CANCELLATION_TOKEN.is_cancelled() {
        let step = STEP.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        if duration_secs < u64::MAX
            && SystemTime::now().duration_since(start).unwrap().as_secs() >= duration_secs
        {
            print_step(sim, step);
            break;
        }

        if step % 1000 == 0 {
            print_step(sim, step);
        }

        handle_actions(sim);

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
