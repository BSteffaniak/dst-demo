use dst_demo_simulator_harness::{rand::Rng as _, turmoil::Sim};

use crate::{ACTIONS, Action, RNG, SIMULATOR_CANCELLATION_TOKEN};

/// # Panics
///
/// * If `CANCELLATION_TOKEN` `Mutex` fails to lock
pub fn start(sim: &mut Sim<'_>) {
    sim.client("McHealer", {
        async move {
            loop {
                tokio::select! {
                    () = SIMULATOR_CANCELLATION_TOKEN.cancelled() => {
                        break;
                    }
                    () = tokio::time::sleep(std::time::Duration::from_secs(RNG.lock().unwrap().gen_range(0..60))) => {}
                }

                ACTIONS.lock().unwrap().push_back(Action::Bounce);
            }

            Ok(())
        }
    });
}
