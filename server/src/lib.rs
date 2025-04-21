#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    str::{self, FromStr as _},
    string::FromUtf8Error,
    sync::LazyLock,
};

use dst_demo_random::Rng;
use dst_demo_tcp::{GenericTcpListener, TcpListener, TcpStream};
use strum::{AsRefStr, EnumString, ParseError};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

pub static SERVER_CANCELLATION_TOKEN: LazyLock<CancellationToken> =
    LazyLock::new(CancellationToken::new);

static RNG: LazyLock<Rng> = LazyLock::new(Rng::new);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[error(transparent)]
    FromUtf8(#[from] FromUtf8Error),
    #[error(transparent)]
    Parse(#[from] ParseError),
    #[error(transparent)]
    Tcp(#[from] dst_demo_tcp::Error),
}

#[derive(Debug, EnumString, AsRefStr)]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum ServerAction {
    Health,
    GenerateRandomNumber,
    Close,
    Exit,
}

/// # Errors
///
/// * If the `TcpListener` fails to bind
///
/// # Panics
///
/// * If the ctrl-c handler fails to be initialized
pub async fn run(addr: impl Into<String>) -> Result<(), Error> {
    let addr = addr.into();
    let listener = TcpListener::bind(&addr).await?;
    log::info!("Server listening on {addr}");

    SERVER_CANCELLATION_TOKEN
        .run_until_cancelled(async move {
            while let Ok((mut stream, _addr)) = listener.accept().await {
                let mut message = String::new();

                loop {
                    let Some(action) = read_message(&mut message, &mut stream).await? else {
                        break;
                    };

                    log::debug!("parsing action={action}");
                    let Ok(action) = ServerAction::from_str(&action).inspect_err(|_| {
                        log::error!("Invalid action '{action}'");
                    }) else {
                        continue;
                    };

                    match action {
                        ServerAction::Health => {
                            log::info!("received health action");
                            health(&mut stream).await?;
                        }
                        ServerAction::GenerateRandomNumber => {
                            log::info!("received generate_random_number action");
                            generate_random_number(&mut stream).await?;
                        }
                        ServerAction::Close => {
                            log::info!("received close action");
                            break;
                        }
                        ServerAction::Exit => {
                            log::info!("received exit action");
                            SERVER_CANCELLATION_TOKEN.cancel();
                            break;
                        }
                    }
                }
            }
            Ok::<_, Error>(())
        })
        .await
        .transpose()
        .unwrap();

    Ok(())
}

async fn read_message(
    message: &mut String,
    stream: &mut TcpStream,
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

async fn health(stream: &mut TcpStream) -> Result<(), std::io::Error> {
    stream.write_all(b"healthy\0").await?;
    log::debug!("responded with \"healthy\"");
    Ok(())
}

async fn generate_random_number(stream: &mut TcpStream) -> Result<(), std::io::Error> {
    let number = RNG.next_u64().checked_add(10000000000000000).unwrap();
    let mut bytes = number.to_string().into_bytes();
    bytes.push(0_u8);
    stream.write_all(&bytes).await?;
    Ok(())
}
