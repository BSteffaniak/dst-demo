use dst_demo_simulator_harness::turmoil::Sim;
use tokio::io::AsyncWriteExt;

use crate::{
    SIMULATOR_CANCELLATION_TOKEN,
    host::server::{HOST, PORT},
    read_message, try_connect,
};

pub fn start(sim: &mut Sim<'_>) {
    let addr = format!("{HOST}:{PORT}");

    sim.client("McGenerateRandomNumber", async move {
        loop {
            static TIMEOUT: u64 = 100;

            log::debug!("generating random number");

            tokio::select! {
                resp = gen_random_number(&addr) => {
                    resp?;
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
                () = SIMULATOR_CANCELLATION_TOKEN.cancelled() => {
                    break;
                }
                () = tokio::time::sleep(std::time::Duration::from_secs(TIMEOUT)) => {
                    return Err(Box::new(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        format!("Failed to get random number response within {TIMEOUT} seconds")
                    )) as Box<dyn std::error::Error>);
                }
            }
        }

        Ok(())
    });
}

async fn gen_random_number(addr: &str) -> Result<(), Box<dyn std::error::Error>> {
    let response = loop {
        log::debug!("[Client] Connecting to server...");
        let mut stream = match try_connect(addr, 1).await {
            Ok(stream) => stream,
            Err(e) => {
                log::error!("[Client] Failed to connect to server: {e:?}");
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                continue;
            }
        };
        log::debug!("[Client] Connected!");
        match stream.write_all(b"GENERATE_RANDOM_NUMBER\0").await {
            Ok(resp) => resp,
            Err(e) => {
                log::error!("failed to make http_request: {e:?}");
                continue;
            }
        }

        let Ok(Some(resp)) = read_message(&mut String::new(), Box::pin(&mut stream)).await else {
            log::debug!("failed to receive random number response");
            continue;
        };

        log::debug!("Received response={resp}");

        break resp;
    };

    response.parse::<u64>().expect("Invalid random number");

    Ok(())
}
