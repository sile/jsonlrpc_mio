use std::{collections::VecDeque, net::SocketAddr};

use jsonlrpc::ResponseObject;
use mio::{event::Event, net::TcpStream, Interest, Poll, Token};
use serde::Serialize;

use crate::connection::{Connection, ConnectionState};

/// RPC client.
#[derive(Debug)]
pub struct RpcClient {
    server_addr: SocketAddr,
    token: Token,
    connection: Option<Connection>,
    responses: VecDeque<ResponseObject>,
}

impl RpcClient {
    /// Makes a new instance of [`RpcClient`].
    ///
    /// If not already connected, this client will establish a connection to the specified server when [`RpcClient::send()`] is called.
    pub fn new(token: Token, server_addr: SocketAddr) -> Self {
        Self {
            server_addr,
            token,
            connection: None,
            responses: VecDeque::new(),
        }
    }

    /// Returns the address of the RPC server to which this client sends requests.
    pub fn server_addr(&self) -> SocketAddr {
        self.server_addr
    }

    /// Returns the `mio` token assigned to this client.
    pub fn token(&self) -> Token {
        self.token
    }

    /// Starts sending a JSON-RPC request to the RPC server.
    pub fn send<T: Serialize>(&mut self, poller: &mut Poll, request: &T) -> serde_json::Result<()> {
        if self.connection.is_none() {
            self.responses.clear();

            let mut stream = TcpStream::connect(self.server_addr).map_err(serde_json::Error::io)?;
            poller
                .registry()
                .register(&mut stream, self.token, Interest::WRITABLE)
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
            .send(poller, request)
            .map_err(|e| self.handle_error(e))
    }

    /// Returns the number of bytes enqueued by [`RpcClient::send()`] that have not yet been written to the TCP socket (e.g., as the send buffer is full).
    pub fn queued_bytes_len(&self) -> usize {
        self.connection.as_ref().map_or(0, |c| c.queued_bytes_len())
    }

    /// Takes a JSON-RPC response from the receive queue.
    pub fn try_recv(&mut self) -> Option<ResponseObject> {
        self.responses.pop_front()
    }

    /// Handles an `mio` event.
    pub fn handle_event(&mut self, poller: &mut Poll, event: &Event) -> serde_json::Result<()> {
        let Some(c) = &mut self.connection else {
            return Ok(());
        };
        c.handle_event(poller, event, |c, _poller| {
            let response = c.stream_mut().read_value()?;
            self.responses.push_back(response);
            Ok(())
        })
        .map_err(|e| self.handle_error(e))
    }

    /// Returns a reference to the internal TCP connection.
    pub fn connection(&self) -> Option<&Connection> {
        self.connection.as_ref()
    }

    /// Closes the internal TCP connection if it has been established.
    pub fn close(&mut self, poller: &mut Poll) {
        let Some(mut c) = self.connection.take() else {
            return;
        };
        c.close(poller);
    }

    fn handle_error(&mut self, error: serde_json::Error) -> serde_json::Error {
        if error.is_io() {
            self.connection = None;
        }
        error
    }
}
