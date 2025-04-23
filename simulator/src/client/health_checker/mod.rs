use dst_demo_simulator_harness::turmoil::Sim;
use plan::{HealthCheckInteractionPlan, Interaction};
use tokio::io::AsyncWriteExt;

pub mod plan;

use crate::{SIMULATOR_CANCELLATION_TOKEN, plan::InteractionPlan as _, read_message, try_connect};

pub fn start(sim: &mut Sim<'_>) {
    let mut plan = HealthCheckInteractionPlan::new().with_gen_interactions(1000);

    sim.client("HealthCheck", async move {
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
        Interaction::HealthCheck(host) => {
            log::info!("perform_interaction: checking health for host={host}");
            health_check(host).await?;
        }
    }

    Ok(())
}

async fn health_check(host: &str) -> Result<(), Box<dyn std::error::Error>> {
    static TIMEOUT: u64 = 10;

    tokio::select! {
        resp = assert_health(host) => {
            resp?;
        }
        () = tokio::time::sleep(std::time::Duration::from_secs(TIMEOUT)) => {
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!("Failed to get healthy response within {TIMEOUT} seconds")
            )) as Box<dyn std::error::Error>);
        }
    }

    Ok(())
}

async fn assert_health(host: &str) -> Result<(), Box<dyn std::error::Error>> {
    let response = loop {
        log::trace!("[Client] Connecting to server...");
        let mut stream = match try_connect(host, 1).await {
            Ok(stream) => stream,
            Err(e) => {
                log::error!("[Client] Failed to connect to server: {e:?}");
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                continue;
            }
        };
        log::trace!("[Client] Connected!");
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
