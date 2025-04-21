#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{pin::Pin, string::FromUtf8Error, sync::LazyLock};

use clap::Parser;
use rustyline::{DefaultEditor, error::ReadlineError};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    task::JoinError,
};
use tokio_util::sync::CancellationToken;

pub static CANCELLATION_TOKEN: LazyLock<CancellationToken> = LazyLock::new(CancellationToken::new);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[error(transparent)]
    FromUtf8(#[from] FromUtf8Error),
    #[error(transparent)]
    Join(#[from] JoinError),
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(index = 1)]
    addr: String,
}

#[tokio::main(flavor = "multi_thread", worker_threads = 10)]
async fn main() -> Result<(), Error> {
    ctrlc::set_handler(move || {
        log::debug!("Received ctrl+c. shutting down...");
        CANCELLATION_TOKEN.cancel();
    })
    .expect("Error setting Ctrl-C handler");

    pretty_env_logger::init();

    let args = Args::parse();
    let addr = args.addr;
    log::info!("Connecting to TCP on addr={addr}...");

    let stream = TcpStream::connect(addr).await?;
    let (mut reader, mut writer) = stream.into_split();

    let reader_handle = CANCELLATION_TOKEN.run_until_cancelled(async move {
        let mut message = String::new();

        loop {
            let Some(response) = read_message(&mut message, Box::pin(&mut reader)).await? else {
                break;
            };

            println!("{response}");
        }

        log::debug!("Finished reading from TCP stream");

        Ok::<_, Error>(())
    });

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    let writer_handle = tokio::spawn(async move {
        while let Some(message) = rx.recv().await {
            writer.write_all(message.as_bytes()).await?;
            writer.write_all(&[0u8]).await?;
            writer.flush().await?;
        }

        Ok::<_, std::io::Error>(())
    });

    // tokio::io::stdin is naturally blocking and non-cancellable, so this
    // is the best we can do
    let read_line_handle = std::thread::spawn(move || {
        let mut rl = DefaultEditor::new().unwrap();

        loop {
            let readline = rl.readline("");

            match readline {
                Ok(message) => {
                    log::debug!("Sending message=\"{message}\"");
                    tx.send(message).unwrap();
                }
                Err(ReadlineError::Interrupted) => {
                    log::debug!("CTRL-C");
                    break;
                }
                Err(ReadlineError::Eof) => {
                    log::debug!("CTRL-D");
                    break;
                }
                Err(err) => {
                    log::error!("Error: {err:?}");
                    break;
                }
            }
        }

        CANCELLATION_TOKEN.cancel();
    });

    reader_handle.await.transpose()?;
    writer_handle.await??;
    read_line_handle.join().unwrap();

    Ok(())
}

async fn read_message(
    message: &mut String,
    mut stream: Pin<Box<impl AsyncReadExt>>,
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
