use dst_demo_simulator_harness::{random::RNG, turmoil::Sim};

use crate::{ACTIONS, Action, SIMULATOR_CANCELLATION_TOKEN};

/// # Panics
///
/// * If `ACTIONS` `Mutex` fails to lock
pub fn start(sim: &mut Sim<'_>) {
    sim.client("FaultInjector", {
        async move {
            loop {
                tokio::select! {
                    () = SIMULATOR_CANCELLATION_TOKEN.cancelled() => {
                        break;
                    }
                    () = tokio::time::sleep(std::time::Duration::from_secs(RNG.gen_range(0..1_000_000))) => {}
                }

                ACTIONS.lock().unwrap().push_back(Action::Bounce);
            }

            Ok(())
        }
    });
}
