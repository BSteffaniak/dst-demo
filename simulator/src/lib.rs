#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    collections::VecDeque,
    pin::Pin,
    string::FromUtf8Error,
    sync::{Arc, LazyLock, Mutex},
};

use dst_demo_simulator_harness::{random::RNG, turmoil::Sim};
use tokio::io::AsyncReadExt;

pub mod client;
pub mod host;
pub mod http;

static ACTIONS: LazyLock<Arc<Mutex<VecDeque<Action>>>> =
    LazyLock::new(|| Arc::new(Mutex::new(VecDeque::new())));

pub static BANKER_COUNT: LazyLock<u64> = LazyLock::new(|| {
    let value = RNG.gen_range(0..20u64);

    std::env::var("SIMULATOR_BANKER_COUNT")
        .ok()
        .map_or(value, |x| x.parse::<u64>().unwrap())
});

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[error(transparent)]
    FromUtf8(#[from] FromUtf8Error),
}

enum Action {
    Bounce(String),
}

/// # Panics
///
/// * If the `ACTIONS` `Mutex` fails to lock
pub fn queue_bounce(host: impl Into<String>) {
    ACTIONS
        .lock()
        .unwrap()
        .push_back(Action::Bounce(host.into()));
}

/// # Panics
///
/// * If `ACTIONS` `Mutex` fails to lock
pub fn handle_actions(sim: &mut Sim<'_>) {
    let actions = ACTIONS.lock().unwrap().drain(..).collect::<Vec<_>>();
    for action in actions {
        match action {
            Action::Bounce(host) => {
                log::debug!("bouncing '{host}'");
                sim.bounce(host);
            }
        }
    }
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
        let count = match stream.read(&mut buf).await {
            Ok(count) => count,
            Err(e) => {
                log::error!("read_message: failed to read from stream: {e:?}");
                break None;
            }
        };
        if count == 0 {
            log::debug!("read_message: received empty response");
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
