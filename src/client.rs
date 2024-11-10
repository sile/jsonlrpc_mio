use std::{collections::VecDeque, net::SocketAddr};

use jsonlrpc::{JsonlStream, ResponseObject};
use mio::{event::Event, net::TcpStream, Poll, Token};
use serde::Serialize;

#[derive(Debug)]
pub struct RpcClientConnection {
    server_addr: SocketAddr,
    token: Token,
    connecting: bool,
    stream: JsonlStream<TcpStream>,
    responses: VecDeque<ResponseObject>,
}

impl RpcClientConnection {
    pub fn connect(server_addr: SocketAddr, token: Token) -> std::io::Result<Self> {
        let stream = TcpStream::connect(server_addr)?;
        stream.set_nodelay(true)?;
        Ok(Self {
            server_addr,
            token,
            connecting: true,
            stream: JsonlStream::new(stream),
            responses: VecDeque::new(),
        })
    }

    pub fn server_addr(&self) -> SocketAddr {
        self.server_addr
    }

    pub fn token(&self) -> Token {
        self.token
    }

    pub fn send<T: Serialize>(&mut self, poller: &mut Poll, message: &T) -> serde_json::Result<()> {
        let queue_size = self.send_queue_byte_size();
        // match self.stream.write_value(message) {
        //     Err(_e) if self.connecting => {
        //         // TODO: self.stream.write_to_buf()
        //         Ok(())
        //     }
        //     Err(e) if e.io_error_kind() == Some(std::io::ErrorKind::WouldBlock) => {
        //         if start_writing {
        //             self.interest = Some(Interest::READABLE | Interest::WRITABLE);
        //         }
        //         Ok(())
        //     }
        //     Err(e) => Err(e.into()),
        //     Ok(_) => Ok(()),
        // }

        // TODO: connect handling
        todo!()
    }

    pub fn send_queue_byte_size(&self) -> usize {
        self.stream.write_buf().len()
    }

    pub fn try_recv(&mut self) -> Option<ResponseObject> {
        self.responses.pop_front()
    }

    pub fn handle_event(&mut self, poller: &mut Poll, event: &Event) -> serde_json::Result<()> {
        Ok(())
    }
}

#[derive(Debug)]
pub struct RpcClient {
    server_addr: SocketAddr,
    token: Token,
    connection: Option<RpcClientConnection>,
}

impl RpcClient {
    pub fn new(server_addr: SocketAddr, token: Token) -> Self {
        Self {
            server_addr,
            token,
            connection: None,
        }
    }

    pub fn server_addr(&self) -> SocketAddr {
        self.server_addr
    }

    pub fn token(&self) -> Token {
        self.token
    }

    pub fn connection(&self) -> Option<&RpcClientConnection> {
        self.connection.as_ref()
    }

    pub fn send<T: Serialize>(&mut self, poller: &mut Poll, message: &T) -> serde_json::Result<()> {
        if self.connection.is_none() {
            self.connection = Some(
                RpcClientConnection::connect(self.server_addr, self.token)
                    .map_err(serde_json::Error::io)?,
            );
        }

        self.connection
            .as_mut()
            .expect("unreachable")
            .send(poller, message)
            .map_err(|e| self.handle_error(poller, e))
    }

    pub fn send_queue_byte_size(&self) -> usize {
        self.connection
            .as_ref()
            .map_or(0, |c| c.send_queue_byte_size())
    }

    pub fn try_recv(&mut self) -> Option<ResponseObject> {
        self.connection.as_mut().map_or(None, |c| c.try_recv())
    }

    pub fn handle_event(&mut self, poller: &mut Poll, event: &Event) -> serde_json::Result<()> {
        let Some(c) = &mut self.connection else {
            return Ok(());
        };
        c.handle_event(poller, event)
            .map_err(|e| self.handle_error(poller, e))
    }

    fn handle_error(&mut self, poller: &mut Poll, error: serde_json::Error) -> serde_json::Error {
        if error.is_io() {
            let mut c = self.connection.take().expect("unreachable");
            let _ = poller.registry().deregister(c.stream.inner_mut());
        }
        error
    }
}
