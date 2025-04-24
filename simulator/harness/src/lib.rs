#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{any::Any, panic::AssertUnwindSafe, sync::LazyLock, time::SystemTime};

use dst_demo_simulator_utils::{SEED, STEP};
use formatting::TimeFormat as _;
use tokio_util::sync::CancellationToken;
use turmoil::Sim;

#[cfg(feature = "random")]
pub use dst_demo_random as random;
pub use dst_demo_simulator_utils as utils;
#[cfg(feature = "tcp")]
pub use dst_demo_tcp as tcp;
#[cfg(feature = "time")]
pub use dst_demo_time as time;
pub use getrandom;
pub use rand;
pub use turmoil;

mod formatting;

pub static SIMULATOR_CANCELLATION_TOKEN: LazyLock<CancellationToken> =
    LazyLock::new(CancellationToken::new);

fn run_info() -> String {
    #[cfg(feature = "time")]
    let extra = {
        use dst_demo_time::simulator::{EPOCH_OFFSET, STEP_MULTIPLIER};

        format!(
            "\n\
            epoch_offset={epoch_offset}\n\
            step_multiplier={step_multiplier}",
            epoch_offset = *EPOCH_OFFSET,
            step_multiplier = *STEP_MULTIPLIER,
        )
    };
    #[cfg(not(feature = "time"))]
    let extra = String::new();

    format!("seed={seed}{extra}", seed = *SEED)
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

/// # Panics
///
/// * If system time went backwards
///
/// # Errors
///
/// * The contents of this function are wrapped in a `catch_unwind` call, so if
///   any panic happens, it will be wrapped into an error on the outer `Result`
/// * If the `Sim` `step` returns an error, we return that in an Ok(Err(e))
pub fn run_simulation(
    sim: &mut Sim<'_>,
    duration_secs: u64,
    on_step: impl Fn(&mut Sim<'_>),
) -> Result<Result<(), Box<dyn std::error::Error>>, Box<dyn Any + Send>> {
    ctrlc::set_handler(move || SIMULATOR_CANCELLATION_TOKEN.cancel())
        .expect("Error setting Ctrl-C handler");

    STEP.store(1, std::sync::atomic::Ordering::SeqCst);

    log::info!("Server simulator starting\n{}", run_info());

    let start = SystemTime::now();

    let resp = std::panic::catch_unwind(AssertUnwindSafe(|| {
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

            on_step(sim);

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

    resp
}
