#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

pub mod bank;

use std::{
    str::{self, FromStr as _},
    string::FromUtf8Error,
    sync::LazyLock,
};

use bank::{Bank, LocalBank, TransactionId};
use dst_demo_async::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use dst_demo_tcp::{GenericTcpListener, GenericTcpStream, TcpListener};
use rust_decimal::Decimal;
use strum::{AsRefStr, EnumString, ParseError};
use tokio_util::sync::CancellationToken;

pub static SERVER_CANCELLATION_TOKEN: LazyLock<CancellationToken> =
    LazyLock::new(CancellationToken::new);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Async(#[from] dst_demo_async::Error),
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
    GetBalance,
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

    let bank = LocalBank::new()?;

    SERVER_CANCELLATION_TOKEN
        .run_until_cancelled(async move {
            while let Ok((stream, addr)) = listener.accept().await {
                let (mut read, mut write) = stream.into_split();
                let mut message = String::new();
                let bank = bank.clone();

                dst_demo_async::task::spawn(async move {
                    while let Ok(Some(action)) = read_message(&mut message, &mut read).await {
                        log::debug!("[{addr}] parsing action={action}");
                        let Ok(action) = ServerAction::from_str(&action).inspect_err(|_| {
                            log::error!("[{addr}] Invalid action '{action}'");
                        }) else {
                            continue;
                        };

                        log::info!("[{addr}] received {action} action");

                        let resp = match action {
                            ServerAction::Health => health(&mut write).await,
                            ServerAction::ListTransactions => {
                                list_transactions(&bank, &mut write).await
                            }
                            ServerAction::GetTransaction => {
                                get_transaction(&bank, &mut message, &mut write, &mut read).await
                            }
                            ServerAction::CreateTransaction => {
                                create_transaction(&bank, &mut message, &mut write, &mut read).await
                            }
                            ServerAction::VoidTransaction => {
                                void_transaction(&bank, &mut message, &mut write, &mut read).await
                            }
                            ServerAction::GetBalance => get_balance(&bank, &mut write).await,
                            ServerAction::Close => {
                                return;
                            }
                            ServerAction::Exit => {
                                SERVER_CANCELLATION_TOKEN.cancel();
                                return;
                            }
                        };

                        if let Err(e) = resp {
                            log::error!("[{addr}] Failed to handle action={action}: {e:?}");
                        }
                    }

                    log::debug!("[{addr}] client connection connection dropped");
                });
            }

            log::debug!("server finished");

            Ok::<_, Error>(())
        })
        .await
        .transpose()
        .unwrap();

    log::debug!("run finished");

    Ok(())
}

async fn read_message(
    message: &mut String,
    reader: &mut (impl AsyncRead + Unpin),
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
        let count = match reader.read(&mut buf).await {
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

async fn write_message(
    message: impl Into<String>,
    stream: &mut (impl AsyncWrite + Unpin),
) -> Result<(), Error> {
    let message = message.into();
    log::debug!("write_message: writing message={message}");
    let mut bytes = message.into_bytes();
    bytes.push(0_u8);
    Ok(stream.write_all(&bytes).await?)
}

async fn list_transactions(
    bank: &impl Bank,
    writer: &mut (impl AsyncWrite + Unpin),
) -> Result<(), Error> {
    let message = {
        let transactions = bank.list_transactions().await?;

        if transactions.is_empty() {
            log::debug!("list_transactions: no transactions");
        }

        transactions
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    };

    write_message(message, writer).await?;

    Ok(())
}

async fn get_transaction(
    bank: &impl Bank,
    message: &mut String,
    writer: &mut (impl AsyncWrite + Unpin),
    reader: &mut (impl AsyncRead + Unpin),
) -> Result<(), Error> {
    write_message("Enter the transaction ID:", writer).await?;
    let Some(message) = read_message(message, reader).await? else {
        use std::io::{Error, ErrorKind};
        return Err(Error::new(
            ErrorKind::NotFound,
            "get_transaction: No message received from TCP client",
        )
        .into());
    };
    let id = message.parse::<TransactionId>()?;
    if let Some(transaction) = bank.get_transaction(id).await? {
        write_message(transaction.to_string(), writer).await?;
    } else {
        write_message("Transaction not found", writer).await?;
    }
    Ok(())
}

async fn create_transaction(
    bank: &impl Bank,
    message: &mut String,
    writer: &mut (impl AsyncWrite + Unpin),
    reader: &mut (impl AsyncRead + Unpin),
) -> Result<(), Error> {
    write_message("Enter the transaction amount:", writer).await?;
    let Some(message) = read_message(message, reader).await? else {
        use std::io::{Error, ErrorKind};
        return Err(Error::new(
            ErrorKind::NotFound,
            "create_transaction: No message received from TCP client",
        )
        .into());
    };
    let transaction = bank
        .create_transaction(Decimal::from_str(&message)?)
        .await?;
    write_message(transaction.to_string(), writer).await?;
    Ok(())
}

async fn void_transaction(
    bank: &impl Bank,
    message: &mut String,
    writer: &mut (impl AsyncWrite + Unpin),
    reader: &mut (impl AsyncRead + Unpin),
) -> Result<(), Error> {
    write_message("Enter the transaction ID:", writer).await?;
    let Some(message) = read_message(message, reader).await? else {
        use std::io::{Error, ErrorKind};
        return Err(Error::new(
            ErrorKind::NotFound,
            "void_transaction: No message received from TCP client",
        )
        .into());
    };
    let id = message.parse::<TransactionId>()?;
    if let Some(transaction) = bank.void_transaction(id).await? {
        write_message(transaction.to_string(), writer).await?;
    } else {
        write_message("Transaction not found", writer).await?;
    }
    Ok(())
}

async fn health(stream: &mut (impl AsyncWrite + Unpin)) -> Result<(), Error> {
    write_message("healthy", stream).await
}

async fn get_balance(
    bank: &impl Bank,
    stream: &mut (impl AsyncWrite + Unpin),
) -> Result<(), Error> {
    let balance = bank.get_balance().await?;
    write_message(format!("${balance}"), stream).await
}
