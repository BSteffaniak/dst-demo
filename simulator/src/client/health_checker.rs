use dst_demo_simulator_harness::turmoil::Sim;
use tokio::io::AsyncWriteExt;

use crate::{
    SIMULATOR_CANCELLATION_TOKEN,
    host::server::{HOST, PORT},
    read_message, try_connect,
};

pub fn start(sim: &mut Sim<'_>) {
    let addr = format!("{HOST}:{PORT}");

    sim.client("McHealthChecker", async move {
        loop {
            static TIMEOUT: u64 = 10;

            log::debug!("checking health");

            tokio::select! {
                resp = assert_health(&addr) => {
                    resp?;
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
                () = SIMULATOR_CANCELLATION_TOKEN.cancelled() => {
                    break;
                }
                () = tokio::time::sleep(std::time::Duration::from_secs(TIMEOUT)) => {
                    return Err(Box::new(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        format!("Failed to get healthy response within {TIMEOUT} seconds")
                    )) as Box<dyn std::error::Error>);
                }
            }
        }

        Ok(())
    });
}

async fn assert_health(addr: &str) -> Result<(), Box<dyn std::error::Error>> {
    let response = loop {
        log::trace!("[Client] Connecting to server...");
        let mut stream = match try_connect(addr, 1).await {
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
