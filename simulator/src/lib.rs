#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    collections::VecDeque,
    pin::Pin,
    string::FromUtf8Error,
    sync::{Arc, LazyLock, Mutex},
    time::Duration,
};

use dst_demo_simulator_harness::{
    rand::{SeedableRng as _, rngs::SmallRng},
    turmoil::{self, Sim, net::TcpStream},
    utils::SEED,
};
use host::server::HOST;
use tokio::io::AsyncReadExt;
use tokio_util::sync::CancellationToken;

pub mod client;
pub mod host;
pub mod http;

pub static SIMULATOR_CANCELLATION_TOKEN: LazyLock<CancellationToken> =
    LazyLock::new(CancellationToken::new);
pub static RNG: LazyLock<Arc<Mutex<SmallRng>>> =
    LazyLock::new(|| Arc::new(Mutex::new(SmallRng::seed_from_u64(*SEED))));
pub static ACTIONS: LazyLock<Arc<Mutex<VecDeque<Action>>>> =
    LazyLock::new(|| Arc::new(Mutex::new(VecDeque::new())));

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[error(transparent)]
    FromUtf8(#[from] FromUtf8Error),
}

pub enum Action {
    Bounce,
}

/// # Panics
///
/// * If `ACTIONS` `Mutex` fails to lock
pub fn handle_actions(sim: &mut Sim<'_>) {
    let actions = ACTIONS.lock().unwrap().drain(..).collect::<Vec<_>>();
    for action in actions {
        match action {
            Action::Bounce => {
                log::info!("bouncing '{HOST}'");
                sim.bounce(HOST);
            }
        }
    }
}

/// # Errors
///
/// * If fails to connect to the TCP stream after `max_attempts` tries
pub async fn try_connect(addr: &str, max_attempts: usize) -> Result<TcpStream, std::io::Error> {
    let mut count = 0;
    Ok(loop {
        tokio::select! {
            resp = turmoil::net::TcpStream::connect(addr) => {
                match resp {
                    Ok(x) => break x,
                    Err(e) => {
                        count += 1;

                        log::debug!("failed to bind tcp: {e:?} (attempt {count}/{max_attempts})");

                        if !matches!(e.kind(), std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::ConnectionReset)
                            || count >= max_attempts
                        {
                            return Err(e);
                        }

                        tokio::time::sleep(Duration::from_millis(5000)).await;
                    }
                }
            }
            () = tokio::time::sleep(Duration::from_millis(5000)) => {
                return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "Timed out after 5000ms"));
            }
        }
    })
}

/// # Errors
///
/// * If there is an IO error
pub async fn read_message(
    message: &mut String,
    mut stream: Pin<Box<impl AsyncReadExt>>,
) -> Result<Option<String>, Error> {
    let mut buf = [0_u8; 1024];

    Ok(loop {
        let Ok(count) = stream
            .read(&mut buf)
            .await
            .inspect_err(|e| log::trace!("Failed to read from stream: {e:?}"))
        else {
            break None;
        };
        if count == 0 {
            break None;
        }
        log::debug!("read count={count}");
        let value = String::from_utf8(buf[..count].to_vec())?;
        message.push_str(&value);

        if let Some(index) = value.chars().position(|x| x == 0 as char) {
            let mut remaining = message.split_off(message.len() - value.len() + index);
            let value = message.clone();
            remaining.remove(0);
            *message = remaining;
            break Some(value);
        }
    })
}
