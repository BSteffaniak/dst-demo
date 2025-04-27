use dst_demo_simulator_harness::{
    CancellableSim, plan::InteractionPlan as _, time::simulator::step_multiplier,
    turmoil::net::TcpStream,
};
use plan::{HealthCheckInteractionPlan, Interaction};
use tokio::io::AsyncWriteExt;

pub mod plan;

use crate::read_message;

pub fn start(sim: &mut impl CancellableSim) {
    let mut plan = HealthCheckInteractionPlan::new().with_gen_interactions(1000);

    sim.client_until_cancelled("HealthCheck", async move {
        loop {
            while let Some(interaction) = plan.step() {
                perform_interaction(interaction).await?;
                tokio::time::sleep(std::time::Duration::from_secs(step_multiplier() * 60)).await;
            }

            plan.gen_interactions(1000);
        }
    });
}

async fn perform_interaction(interaction: &Interaction) -> Result<(), Box<dyn std::error::Error>> {
    log::debug!("perform_interaction: interaction={interaction:?}");

    match interaction {
        Interaction::Sleep(duration) => {
            log::debug!("perform_interaction: sleeping for duration={duration:?}");
            tokio::time::sleep(*duration).await;
        }
        Interaction::HealthCheck(host) => {
            log::debug!("perform_interaction: checking health for host={host}");
            health_check(host).await?;
        }
    }

    Ok(())
}

async fn health_check(host: &str) -> Result<(), Box<dyn std::error::Error>> {
    let timeout = 10 * step_multiplier();

    tokio::select! {
        resp = assert_health(host) => {
            resp?;
        }
        () = tokio::time::sleep(std::time::Duration::from_secs(timeout)) => {
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!("Failed to get healthy response within {timeout} seconds")
            )) as Box<dyn std::error::Error>);
        }
    }

    Ok(())
}

async fn assert_health(host: &str) -> Result<(), Box<dyn std::error::Error>> {
    let response = loop {
        log::trace!("[Health Client] Connecting to server...");
        let mut stream = match TcpStream::connect(host).await {
            Ok(stream) => stream,
            Err(e) => {
                log::debug!("[Health Client] Failed to connect to server: {e:?}");
                tokio::time::sleep(std::time::Duration::from_millis(step_multiplier())).await;
                continue;
            }
        };
        log::trace!("[Health Client] Connected!");
        match stream.write_all(b"HEALTH\0").await {
            Ok(resp) => resp,
            Err(e) => {
                log::error!("failed to make http_request: {e:?}");
                continue;
            }
        }

        let Ok(Some(resp)) = read_message(&mut String::new(), Box::pin(&mut stream)).await else {
            log::debug!("failed to receive healthy response");
            continue;
        };

        log::debug!("Received response={resp}");

        break resp;
    };

    assert!(response == "healthy");

    Ok(())
}
