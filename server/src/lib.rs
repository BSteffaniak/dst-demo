#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

pub mod bank;

use std::{
    str::{self, FromStr as _},
    string::FromUtf8Error,
    sync::LazyLock,
};

use bank::{Bank, LocalBank, Transaction};
use dst_demo_random::Rng;
use dst_demo_tcp::{GenericTcpListener, TcpListener, TcpStream};
use rust_decimal::Decimal;
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
    #[error(transparent)]
    Decimal(#[from] rust_decimal::Error),
    #[error(transparent)]
    Bank(#[from] bank::Error),
    #[error(transparent)]
    ParseInt(#[from] std::num::ParseIntError),
}

#[derive(Debug, EnumString, AsRefStr)]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum ServerAction {
    Health,
    ListTransactions,
    GetTransaction,
    CreateTransaction,
    VoidTransaction,
    GenerateRandomNumber,
    Close,
    Exit,
}

impl std::fmt::Display for ServerAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
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

    let mut bank = LocalBank::new();

    SERVER_CANCELLATION_TOKEN
        .run_until_cancelled(async move {
            while let Ok((mut stream, addr)) = listener.accept().await {
                let mut message = String::new();

                while let Ok(Some(action)) = read_message(&mut message, &mut stream).await {
                    log::debug!("parsing action={action}");
                    let Ok(action) = ServerAction::from_str(&action).inspect_err(|_| {
                        log::error!("Invalid action '{action}'");
                    }) else {
                        continue;
                    };

                    log::info!("received {action} action");

                    let resp = match action {
                        ServerAction::Health => health(&mut stream).await,
                        ServerAction::ListTransactions => {
                            list_transactions(&bank, &mut stream).await
                        }
                        ServerAction::GetTransaction => {
                            get_transaction(&bank, &mut message, &mut stream).await
                        }
                        ServerAction::CreateTransaction => {
                            create_transaction(&mut bank, &mut message, &mut stream).await
                        }
                        ServerAction::VoidTransaction => {
                            void_transaction(&mut bank, &mut message, &mut stream).await
                        }
                        ServerAction::GenerateRandomNumber => {
                            generate_random_number(&mut stream).await
                        }
                        ServerAction::Close => {
                            break;
                        }
                        ServerAction::Exit => {
                            SERVER_CANCELLATION_TOKEN.cancel();
                            break;
                        }
                    };

                    if let Err(e) = resp {
                        log::error!("Failed to handle action={action}: {e:?}");
                    }
                }

                log::debug!("client connection connection dropped with addr={addr}");
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
    if let Some(index) = message.chars().position(|x| x == 0 as char) {
        let mut remaining = message.split_off(index);
        let value = message.clone();
        remaining.remove(0);
        *message = remaining;
        return Ok(Some(value));
    }

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

async fn write_message(message: impl Into<String>, stream: &mut TcpStream) -> Result<(), Error> {
    let message = message.into();
    log::debug!("write_message: writing message={message}");
    let mut bytes = message.into_bytes();
    bytes.push(0_u8);
    Ok(stream.write_all(&bytes).await?)
}

impl std::fmt::Display for &Transaction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "id={} created_at={} amount=${:.2}",
            self.id, self.created_at, self.amount
        ))
    }
}

async fn list_transactions(bank: &impl Bank, stream: &mut TcpStream) -> Result<(), Error> {
    let transactions = bank.list_transactions()?;

    for transaction in transactions {
        write_message(transaction.to_string(), stream).await?;
    }

    Ok(())
}

async fn get_transaction(
    bank: &impl Bank,
    message: &mut String,
    stream: &mut TcpStream,
) -> Result<(), Error> {
    write_message("Enter the transaction ID:", stream).await?;
    let Some(message) = read_message(message, stream).await? else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "No message received from TCP client",
        )
        .into());
    };
    let id = message.parse::<i32>()?;
    if let Some(transaction) = bank.get_transaction(id)? {
        write_message(transaction.to_string(), stream).await?;
    } else {
        write_message("Transaction not found", stream).await?;
    }
    Ok(())
}

async fn create_transaction(
    bank: &mut impl Bank,
    message: &mut String,
    stream: &mut TcpStream,
) -> Result<(), Error> {
    write_message("Enter the transaction amount:", stream).await?;
    let Some(message) = read_message(message, stream).await? else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "No message received from TCP client",
        )
        .into());
    };
    let transaction = bank.create_transaction(Decimal::from_str(&message)?)?;
    write_message(transaction.to_string(), stream).await?;
    Ok(())
}

async fn void_transaction(
    bank: &mut impl Bank,
    message: &mut String,
    stream: &mut TcpStream,
) -> Result<(), Error> {
    write_message("Enter the transaction ID:", stream).await?;
    let Some(message) = read_message(message, stream).await? else {
        use std::io::{Error, ErrorKind};
        return Err(Error::new(ErrorKind::NotFound, "No message received from TCP client").into());
    };
    let id = message.parse::<i32>()?;
    if let Some(transaction) = bank.void_transaction(id)? {
        write_message(transaction.to_string(), stream).await?;
    } else {
        write_message("Transaction not found", stream).await?;
    }
    Ok(())
}

async fn health(stream: &mut TcpStream) -> Result<(), Error> {
    write_message("healthy", stream).await
}

async fn generate_random_number(stream: &mut TcpStream) -> Result<(), Error> {
    let number = RNG.next_u64();
    write_message(number.to_string(), stream).await
}
