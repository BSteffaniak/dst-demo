#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{net::SocketAddr, pin::pin};

use ::tokio::io::{AsyncRead, AsyncWrite};
use async_trait::async_trait;
use thiserror::Error;

#[cfg(feature = "tokio")]
pub mod tokio;

#[cfg(feature = "simulator")]
pub mod simulator;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    IO(#[from] ::std::io::Error),
}

#[async_trait]
pub trait GenericTcpListener: Send + Sync {
    async fn accept(&self) -> Result<(TcpStream, SocketAddr), Error>;
}

pub struct TcpListener(Box<dyn GenericTcpListener>);

#[async_trait]
impl GenericTcpListener for TcpListener {
    async fn accept(&self) -> Result<(TcpStream, SocketAddr), Error> {
        self.0.accept().await
    }
}

impl TcpListener {
    /// # Errors
    ///
    /// * If the generic `TcpListener` fails to bind the address
    ///
    /// # Panics
    ///
    /// * If all TCP backend features are disabled
    #[allow(clippy::unused_async)]
    pub async fn bind(addr: impl Into<String>) -> Result<Self, Error> {
        let addr = addr.into();

        #[cfg(feature = "simulator")]
        if dst_demo_simulator_utils::simulator_enabled() {
            return Ok(Self(Box::new(
                simulator::SimulatorTcpListener::bind(&addr).await?,
            )));
        }

        if cfg!(feature = "tokio") {
            #[cfg(feature = "tokio")]
            {
                Self::bind_tokio(addr).await
            }
            #[cfg(not(feature = "tokio"))]
            unreachable!()
        } else {
            panic!("No HTTP backend feature enabled (addr={addr})");
        }
    }

    /// # Errors
    ///
    /// * If the `tokio::net::TcpListener` fails to bind the address
    #[cfg(feature = "tokio")]
    #[allow(unreachable_code)]
    pub async fn bind_tokio(addr: impl Into<String>) -> Result<Self, Error> {
        let addr = addr.into();

        #[cfg(feature = "simulator")]
        if dst_demo_simulator_utils::simulator_enabled() {
            return Ok(Self(Box::new(
                simulator::SimulatorTcpListener::bind(&addr).await?,
            )));
        }

        Ok(Self(Box::new(::tokio::net::TcpListener::bind(addr).await?)))
    }
}

#[async_trait]
pub trait GenericTcpStream: AsyncRead + AsyncWrite + Send + Sync + Unpin {}

pub struct TcpStream(Box<dyn GenericTcpStream>);

impl GenericTcpStream for TcpStream {}

impl AsyncRead for TcpStream {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut ::tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let this = self.get_mut();
        let inner = &mut this.0;
        let inner = pin!(inner);
        AsyncRead::poll_read(inner, cx, buf)
    }
}

impl AsyncWrite for TcpStream {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        let this = self.get_mut();
        let inner = &mut this.0;
        let inner = pin!(inner);
        AsyncWrite::poll_write(inner, cx, buf)
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        let this = self.get_mut();
        let inner = &mut this.0;
        let inner = pin!(inner);
        AsyncWrite::poll_flush(inner, cx)
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        let this = self.get_mut();
        let inner = &mut this.0;
        let inner = pin!(inner);
        AsyncWrite::poll_shutdown(inner, cx)
    }
}
