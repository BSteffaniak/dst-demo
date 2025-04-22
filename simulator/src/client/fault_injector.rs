use dst_demo_simulator_harness::{random::RNG, turmoil::Sim};

use crate::SIMULATOR_CANCELLATION_TOKEN;

pub fn start(sim: &mut Sim<'_>) {
    sim.client("McFaultInjector", {
        async move {
            loop {
                tokio::select! {
                    () = SIMULATOR_CANCELLATION_TOKEN.cancelled() => {
                        break;
                    }
                    () = tokio::time::sleep(std::time::Duration::from_secs(RNG.gen_range(0..60))) => {}
                }
            }

            Ok(())
        }
    });
}
