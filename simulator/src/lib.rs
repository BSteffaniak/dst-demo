#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    collections::VecDeque,
    pin::Pin,
    string::FromUtf8Error,
    sync::{Arc, LazyLock, Mutex, RwLock},
};

use dst_demo_simulator_harness::{CancellableSim, random::RNG};
use tokio::io::AsyncReadExt;

pub mod client;
pub mod host;
pub mod http;

static ACTIONS: LazyLock<Arc<Mutex<VecDeque<Action>>>> =
    LazyLock::new(|| Arc::new(Mutex::new(VecDeque::new())));

static BANKER_COUNT: LazyLock<RwLock<Option<u64>>> = LazyLock::new(|| RwLock::new(None));

fn gen_banker_count() -> u64 {
    let value = RNG.gen_range(1..30u64);

    std::env::var("SIMULATOR_BANKER_COUNT")
        .ok()
        .map_or(value, |x| x.parse::<u64>().unwrap())
}

/// # Panics
///
/// * If the `BANKER_COUNT` `RwLock` fails to write to
pub fn reset_banker_count() -> u64 {
    let value = gen_banker_count();
    *BANKER_COUNT.write().unwrap() = Some(value);
    value
}

/// # Panics
///
/// * If the `BANKER_COUNT` `RwLock` fails to read from
#[must_use]
pub fn banker_count() -> u64 {
    let value = { *BANKER_COUNT.read().unwrap() };
    value.unwrap_or_else(|| {
        let value = gen_banker_count();
        *BANKER_COUNT.write().unwrap() = Some(value);
        value
    })
}

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
pub fn handle_actions(sim: &mut impl CancellableSim) {
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
        log::trace!("read count={count}");
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
