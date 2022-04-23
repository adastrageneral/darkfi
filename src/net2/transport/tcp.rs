use async_std::{
    net::{TcpListener, TcpStream},
    sync::Arc,
};
use std::{io, net::SocketAddr, pin::Pin};

use async_trait::async_trait;
use futures::prelude::*;
use log::debug;
use socket2::{Domain, Socket, Type};
use url::Url;

use super::{Transport, TransportError};

#[derive(Clone)]
pub struct TcpTransport {
    /// TTL to set for opened sockets, or `None` for default
    ttl: Option<u32>,
    /// Size of the listen backlog for listen sockets
    backlog: i32,
}

#[async_trait]
impl Transport for TcpTransport {
    type Acceptor = TcpListener;
    type Connector = TcpStream;

    type Error = io::Error;

    type Listener = Pin<
        Box<dyn Future<Output = Result<Self::Acceptor, TransportError<Self::Error>>> + Send + Sync>,
    >;
    type Dial = Pin<
        Box<
            dyn Future<Output = Result<Self::Connector, TransportError<Self::Error>>> + Send + Sync,
        >,
    >;

    fn listen_on(self, url: Url) -> Result<Self::Listener, TransportError<Self::Error>> {
        if url.scheme() != "tcp" {
            return Err(TransportError::AddrNotSupported(url))
        }

        let socket_addr = url.socket_addrs(|| None)?[0];
        debug!(target: "tcptransport", "listening on {}", socket_addr);
        Ok(Box::pin(self.do_listen(socket_addr)))
    }

    fn dial(self, url: Url) -> Result<Self::Dial, TransportError<Self::Error>> {
        if url.scheme() != "tcp" {
            return Err(TransportError::AddrNotSupported(url))
        }

        let socket_addr = url.socket_addrs(|| None)?[0];
        debug!(target: "tcptransport", "dialing {}", socket_addr);
        Ok(Box::pin(self.do_dial(socket_addr)))
    }

    fn new(ttl: Option<u32>, backlog: i32) -> Self {
        Self { ttl, backlog }
    }

    async fn accept(
        listener: Arc<Self::Acceptor>,
    ) -> Result<Self::Connector, TransportError<Self::Error>> {
        Ok(listener.accept().await?.0)
    }
}

impl TcpTransport {
    fn create_socket(&self, socket_addr: SocketAddr) -> io::Result<Socket> {
        let domain = if socket_addr.is_ipv4() { Domain::IPV4 } else { Domain::IPV6 };
        let socket = Socket::new(domain, Type::STREAM, Some(socket2::Protocol::TCP))?;

        if socket_addr.is_ipv6() {
            socket.set_only_v6(true)?;
        }

        if let Some(ttl) = self.ttl {
            socket.set_ttl(ttl)?;
        }

        Ok(socket)
    }

    async fn do_listen(
        self,
        socket_addr: SocketAddr,
    ) -> Result<TcpListener, TransportError<io::Error>> {
        let socket = self.create_socket(socket_addr)?;
        socket.bind(&socket_addr.into())?;
        socket.listen(self.backlog)?;
        socket.set_nonblocking(true)?;
        Ok(TcpListener::from(std::net::TcpListener::from(socket)))
    }

    async fn do_dial(
        self,
        socket_addr: SocketAddr,
    ) -> Result<TcpStream, TransportError<io::Error>> {
        let socket = self.create_socket(socket_addr)?;
        socket.set_nonblocking(true)?;

        match socket.connect(&socket_addr.into()) {
            Ok(()) => {}
            Err(err) if err.raw_os_error() == Some(libc::EINPROGRESS) => {}
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {}
            Err(err) => return Err(TransportError::Other(err)),
        };

        let stream = TcpStream::from(std::net::TcpStream::from(socket));
        Ok(stream)
    }
}