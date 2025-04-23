use dst_demo_server::ServerAction;
use dst_demo_simulator_harness::turmoil::{Sim, net::TcpStream};
use plan::{BankerInteractionPlan, Interaction};
use tokio::io::AsyncWriteExt as _;

mod plan;

use crate::{
    SIMULATOR_CANCELLATION_TOKEN,
    host::server::{HOST, PORT},
    plan::InteractionPlan as _,
    try_connect,
};

/// # Panics
///
/// * If `CANCELLATION_TOKEN` `Mutex` fails to lock
pub fn start(sim: &mut Sim<'_>) {
    let addr = format!("{HOST}:{PORT}");

    log::debug!("Generating initial test plan");

    let mut plan = BankerInteractionPlan::new().with_gen_interactions(1000);

    sim.client("Banker", async move {
        SIMULATOR_CANCELLATION_TOKEN
            .run_until_cancelled(async move {
                loop {
                    while let Some(interaction) = plan.step() {
                        static TIMEOUT: u64 = 10;

                        #[allow(clippy::cast_possible_truncation)]
                        let interaction_timeout = TIMEOUT
                            + if let Interaction::Sleep(duration) = &interaction {
                                duration.as_millis() as u64
                            } else {
                                0
                            };

                        tokio::select! {
                            resp = perform_interaction(&addr, interaction) => {
                                resp?;
                            }
                            () = tokio::time::sleep(std::time::Duration::from_secs(interaction_timeout)) => {
                                return Err(Box::new(std::io::Error::new(
                                    std::io::ErrorKind::TimedOut,
                                    format!("Failed to get interaction response within {interaction_timeout} seconds")
                                )) as Box<dyn std::error::Error>);
                            }
                        }
                    }

                    plan.gen_interactions(1000);
                }
            })
            .await
            .transpose()
            .map(|x| x.unwrap_or(()))
    });
}

async fn send_action(stream: &mut TcpStream, action: ServerAction) -> bool {
    log::debug!("send_action: action={action}");
    let success = send_message(stream, action.to_string()).await;
    log::debug!("send_action: sent action={action} success={success}");
    success
}

async fn send_message(stream: &mut TcpStream, message: impl Into<String>) -> bool {
    let message = message.into();
    log::debug!("send_message: message={message}");
    let mut bytes = message.clone().into_bytes();
    bytes.push(0_u8);
    match stream.write_all(&bytes).await {
        Ok(resp) => resp,
        Err(e) => {
            log::error!("failed to make tcp_request: {e:?}");
            return false;
        }
    }
    log::debug!("send_message: sent message={message} success=true");

    true
}

async fn perform_interaction(
    addr: &str,
    interaction: &Interaction,
) -> Result<(), Box<dyn std::error::Error>> {
    log::debug!("perform_interaction: interaction={interaction:?}");

    if let Interaction::Sleep(duration) = interaction {
        log::debug!("perform_interaction: sleeping for duration={duration:?}");
        tokio::time::sleep(*duration).await;
        return Ok(());
    }

    loop {
        log::trace!("[Banker Client] Connecting to server...");
        let mut stream = match try_connect(addr, 1).await {
            Ok(stream) => stream,
            Err(e) => {
                log::error!("[Banker Client] Failed to connect to server: {e:?}");
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                continue;
            }
        };
        log::trace!("[Banker Client] Connected!");

        match interaction {
            Interaction::Sleep(..) => {
                unreachable!();
            }
            Interaction::ListTransactions => {
                if !send_action(&mut stream, ServerAction::ListTransactions).await {
                    log::debug!("perform_interaction: ListTransactions failed to send");
                    continue;
                }
            }
            Interaction::GetTransaction { id } => {
                if !send_action(&mut stream, ServerAction::GetTransaction).await {
                    log::debug!("perform_interaction: GetTransaction failed to send");
                    continue;
                }
                if !send_message(&mut stream, id.to_string()).await {
                    log::debug!("perform_interaction: GetTransaction id failed to send");
                    continue;
                }
            }
            Interaction::CreateTransaction { amount } => {
                if !send_action(&mut stream, ServerAction::CreateTransaction).await {
                    log::debug!("perform_interaction: CreateTransaction failed to send");
                    continue;
                }
                if !send_message(&mut stream, amount.to_string()).await {
                    log::debug!("perform_interaction: CreateTransaction id failed to send");
                    continue;
                }
            }
            Interaction::VoidTransaction { id } => {
                if !send_action(&mut stream, ServerAction::VoidTransaction).await {
                    log::debug!("perform_interaction: VoidTransaction failed to send");
                    continue;
                }
                if !send_message(&mut stream, id.to_string()).await {
                    log::debug!("perform_interaction: VoidTransaction id failed to send");
                    continue;
                }
            }
        }

        break;
    }

    Ok(())
}
