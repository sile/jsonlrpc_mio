use std::{collections::VecDeque, net::SocketAddr};

use jsonlrpc::ResponseObject;
use mio::{event::Event, net::TcpStream, Interest, Poll, Token};
use serde::Serialize;

use crate::connection::{Connection, ConnectionState};

#[derive(Debug)]
pub struct RpcClient {
    server_addr: SocketAddr,
    token: Token,
    connection: Option<Connection>,
    responses: VecDeque<ResponseObject>,
}

impl RpcClient {
    pub fn new(token: Token, server_addr: SocketAddr) -> Self {
        Self {
            server_addr,
            token,
            connection: None,
            responses: VecDeque::new(),
        }
    }

    pub fn server_addr(&self) -> SocketAddr {
        self.server_addr
    }

    pub fn token(&self) -> Token {
        self.token
    }

    pub fn connection(&self) -> Option<&Connection> {
        self.connection.as_ref()
    }

    pub fn send<T: Serialize>(&mut self, poller: &mut Poll, message: &T) -> serde_json::Result<()> {
        if self.connection.is_none() {
            self.responses.clear();

            let mut stream = TcpStream::connect(self.server_addr).map_err(serde_json::Error::io)?;
            poller
                .registry()
                .register(
                    &mut stream,
                    self.token,
                    Interest::READABLE | Interest::WRITABLE,
                )
                .map_err(serde_json::Error::io)?;
            self.connection = Some(Connection::new(
                self.token,
                stream,
                ConnectionState::Connecting,
            ));
        }

        self.connection
            .as_mut()
            .expect("unreachable")
            .send(poller, message)
            .map_err(|e| self.handle_error(e))
    }

    pub fn queued_bytes_len(&self) -> usize {
        self.connection.as_ref().map_or(0, |c| c.queued_bytes_len())
    }

    pub fn try_recv(&mut self) -> Option<ResponseObject> {
        self.responses.pop_front()
    }

    pub fn handle_event(&mut self, poller: &mut Poll, event: &Event) -> serde_json::Result<()> {
        let Some(c) = &mut self.connection else {
            return Ok(());
        };
        c.handle_event(poller, event, |stream| {
            let response = stream.read_value()?;
            self.responses.push_back(response);
            Ok(())
        })
        .map_err(|e| self.handle_error(e))
    }

    // TOOD: fn close()

    fn handle_error(&mut self, error: serde_json::Error) -> serde_json::Error {
        if error.is_io() {
            self.connection = None;
        }
        error
    }
}
