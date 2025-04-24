use dst_demo_simulator_harness::{SIMULATOR_CANCELLATION_TOKEN, turmoil::Sim};
use plan::{FaultInjectionInteractionPlan, Interaction};

pub mod plan;

use crate::{host::server::CANCELLATION_TOKEN, plan::InteractionPlan as _, queue_bounce};

/// # Panics
///
/// * If `ACTIONS` `Mutex` fails to lock
pub fn start(sim: &mut Sim<'_>) {
    log::debug!("Generating initial test plan");

    let mut plan = FaultInjectionInteractionPlan::new().with_gen_interactions(1000);

    sim.client("FaultInjector", async move {
        SIMULATOR_CANCELLATION_TOKEN
            .run_until_cancelled(async move {
                loop {
                    while let Some(interaction) = plan.step() {
                        perform_interaction(interaction).await?;
                    }

                    plan.gen_interactions(1000);
                }
            })
            .await
            .transpose()
            .map(|x| x.unwrap_or(()))
    });
}

async fn perform_interaction(interaction: &Interaction) -> Result<(), Box<dyn std::error::Error>> {
    log::debug!("perform_interaction: interaction={interaction:?}");

    match interaction {
        Interaction::Sleep(duration) => {
            log::debug!("perform_interaction: sleeping for duration={duration:?}");
            tokio::time::sleep(*duration).await;
        }
        Interaction::Bounce(host) => {
            log::debug!("perform_interaction: queueing bouncing '{host}'");
            if let Some(token) = { CANCELLATION_TOKEN.lock().unwrap().clone() } {
                token.cancel();
            }
            queue_bounce(host);
        }
    }

    Ok(())
}
