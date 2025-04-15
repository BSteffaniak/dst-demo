use std::{net::SocketAddr, pin::pin};

use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::{GenericTcpListener, GenericTcpStream, TcpStream};

pub struct SimulatorTcpListener(turmoil::net::TcpListener);

impl SimulatorTcpListener {
    /// # Errors
    ///
    /// * If the `turmoil::new::TcpListener` fails to bind the address
    pub async fn bind(addr: &str) -> Result<Self, crate::Error> {
        Ok(Self(turmoil::net::TcpListener::bind(addr).await?))
    }
}

#[async_trait]
impl GenericTcpListener for SimulatorTcpListener {
    async fn accept(&self) -> Result<(TcpStream, SocketAddr), crate::Error> {
        let (stream, addr) = self.0.accept().await?;
        Ok((TcpStream(Box::new(SimulatorTcpStream(stream))), addr))
    }
}

pub struct SimulatorTcpStream(turmoil::net::TcpStream);

#[async_trait]
impl GenericTcpStream for SimulatorTcpStream {}

impl AsyncRead for SimulatorTcpStream {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let this = self.get_mut();
        let inner = &mut this.0;
        let inner = pin!(inner);
        AsyncRead::poll_read(inner, cx, buf)
    }
}

impl AsyncWrite for SimulatorTcpStream {
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
