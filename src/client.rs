use std::net::SocketAddr;

use mio::{event::Event, Poll};

#[derive(Debug)]
pub struct RpcClient {
    server_addr: SocketAddr,
}

impl RpcClient {
    pub fn new(server_addr: SocketAddr) -> Self {
        Self { server_addr }
    }

    pub fn server_addr(&self) -> SocketAddr {
        self.server_addr
    }

    pub fn handle_event(&mut self, poller: &mut Poll, event: &Event) {}
}
