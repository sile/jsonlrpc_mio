use std::{net::SocketAddr, time::Duration};

use jsonlrpc::{JsonlStream, RequestId};
use mio::{net::TcpStream, Token};
use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub struct JsonRpcNode {}

impl JsonRpcNode {
    // TODO: options
    pub fn start(listen_addr: SocketAddr) {}

    pub fn send_request<T: Serialize>(
        &mut self,
        peer: SocketAddr,
        method: &str,
        params: &T,
    ) -> RequestId {
        todo!()
    }

    pub fn send_request_without_params(&mut self, peer: SocketAddr, method: &str) -> RequestId {
        todo!()
    }

    pub fn send_notification<T: Serialize>(&mut self, peer: SocketAddr, method: &str, params: &T) {
        todo!()
    }

    pub fn poll<F, T>(&mut self, on_readable: F, timeout: Option<Duration>)
    where
        F: FnMut(&mut Connection) -> serde_json::Result<()>,
        T: for<'de> Deserialize<'de>,
    {
    }

    // TODO: -> ConnectionId
    pub fn connect(&mut self, peer: SocketAddr) -> PeerId {
        todo!()
    }

    pub fn get_connection(&self, peer_addr: SocketAddr) -> Option<&Connection> {
        todo!()
    }
}

#[derive(Debug)]
pub struct From {
    pub request_id: RequestId,
    pub token: Token, // TODO: private
}

// TODO: remove
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PeerId(pub u64);

// TODO: Token or ConnectionId

#[derive(Debug)]
pub struct Connection {
    peer_addr: SocketAddr,
    stream: JsonlStream<TcpStream>,
}

impl Connection {
    pub fn peer_addr(&self) -> SocketAddr {
        self.peer_addr
    }

    pub fn recv<T>(&mut self) -> serde_json::Result<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        todo!()
    }

    pub fn send<T: Serialize>(&mut self, msg: &T) -> serde_json::Result<()> {
        self.stream.write_object(msg)
    }

    pub fn send_queue_size(&self) -> usize {
        self.stream.write_buf().len()
    }
}
