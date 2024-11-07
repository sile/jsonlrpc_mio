use std::{collections::VecDeque, net::SocketAddr};

use jsonlrpc::{JsonlStream, ResponseObject};
use mio::{event::Event, net::TcpStream, Poll};
use serde::Serialize;

#[derive(Debug)]
pub struct RpcClient {
    server_addr: SocketAddr,
    is_connected: bool,
    stream: JsonlStream<TcpStream>,
    responses: VecDeque<ResponseObject>,
}

impl RpcClient {
    pub fn connect(server_addr: SocketAddr) -> std::io::Result<Self> {
        let stream = TcpStream::connect(server_addr)?;
        stream.set_nodelay(true)?;
        Ok(Self {
            server_addr,
            is_connected: false,
            stream: JsonlStream::new(stream),
            responses: VecDeque::new(),
        })
    }

    pub fn server_addr(&self) -> SocketAddr {
        self.server_addr
    }

    pub fn send<T: Serialize>(&mut self, poller: &mut Poll, message: &T) -> serde_json::Result<()> {
        // TODO: connect handling
        todo!()
    }

    pub fn send_queue_size(&self) -> usize {
        self.stream.write_buf().len()
    }

    // TODO: disconnected handling
    pub fn try_recv(&mut self) -> Option<ResponseObject> {
        self.responses.pop_front()
    }

    pub fn handle_event<F>(&mut self, poller: &mut Poll, event: &Event) {}
}

// TODO: RpcClientAny
