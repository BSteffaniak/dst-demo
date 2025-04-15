use std::net::SocketAddr;

use async_trait::async_trait;

use crate::{GenericTcpListener, GenericTcpStream, TcpStream};

#[async_trait]
impl GenericTcpListener for ::tokio::net::TcpListener {
    async fn accept(&self) -> Result<(TcpStream, SocketAddr), crate::Error> {
        let (stream, addr) = self.accept().await?;
        Ok((TcpStream(Box::new(stream)), addr))
    }
}

#[async_trait]
impl GenericTcpStream for tokio::net::TcpStream {}
