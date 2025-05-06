use dst_demo_simulator_harness::{Sim, plan::InteractionPlan as _};
use plan::{FaultInjectionInteractionPlan, Interaction};

pub mod plan;

use crate::queue_bounce;

pub fn start(sim: &mut impl Sim) {
    log::debug!("Generating initial test plan");

    let mut plan = FaultInjectionInteractionPlan::new().with_gen_interactions(1000);

    sim.client("fault_injector", async move {
        loop {
            while let Some(interaction) = plan.step() {
                perform_interaction(interaction).await?;
            }

            plan.gen_interactions(1000);
        }
    });
}

async fn perform_interaction(
    interaction: &Interaction,
) -> Result<(), Box<dyn std::error::Error + Send>> {
    log::debug!("perform_interaction: interaction={interaction:?}");

    match interaction {
        Interaction::Sleep(duration) => {
            log::debug!("perform_interaction: sleeping for duration={duration:?}");
            dst_demo_async::time::sleep(*duration).await;
        }
        Interaction::Bounce(host) => {
            log::debug!("perform_interaction: queueing bouncing '{host}'");
            queue_bounce(host);
        }
    }

    Ok(())
}
