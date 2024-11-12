use std::{
    collections::{HashMap, VecDeque},
    io::ErrorKind,
    marker::PhantomData,
    net::SocketAddr,
};

use jsonlrpc::{ErrorCode, ErrorObject, RequestObject, ResponseObject};
use mio::{
    event::Event,
    net::{TcpListener, TcpStream},
    Interest, Poll, Token,
};
use serde::{Deserialize, Serialize};

use crate::connection::{Connection, ConnectionState};

/// RPC server.
#[derive(Debug)]
pub struct RpcServer<REQ = RequestObject> {
    listen_addr: SocketAddr,
    listener: TcpListener,
    token_min: Token,
    token_max: Token,
    next_token: Token,
    connections: HashMap<Token, Connection>,
    requests: VecDeque<(From, REQ)>,
    _request: PhantomData<REQ>,
}

impl<REQ> RpcServer<REQ>
where
    REQ: for<'de> Deserialize<'de>,
{
    /// Starts an [`RpcServer`] that listens on the specified address.
    pub fn start(
        poller: &mut Poll,
        listen_addr: SocketAddr,
        token_min: Token,
        token_max: Token,
    ) -> std::io::Result<Self> {
        if token_min > token_max {
            return Err(std::io::Error::new(
                ErrorKind::InvalidInput,
                "Empty token range",
            ));
        }

        let mut listener = TcpListener::bind(listen_addr)?;
        let listen_addr = listener.local_addr()?;
        poller
            .registry()
            .register(&mut listener, token_min, Interest::READABLE)?;
        Ok(Self {
            listen_addr,
            listener,
            token_min,
            token_max,
            next_token: Token(token_min.0 + 1),
            connections: HashMap::new(),
            requests: VecDeque::new(),
            _request: PhantomData,
        })
    }

    /// Returns the address on which this server is listening.
    pub fn listen_addr(&self) -> SocketAddr {
        self.listen_addr
    }

    /// Takes a JSON-RPC request from the receive queue.
    pub fn try_recv(&mut self) -> Option<(From, REQ)> {
        self.requests.pop_front()
    }

    /// Sends a JSON-RPC response.
    pub fn reply<T: Serialize>(
        &mut self,
        poller: &mut Poll,
        from: From,
        response: &T,
    ) -> std::io::Result<bool> {
        let Some(connection) = self.connections.get_mut(&from.token) else {
            return Ok(false);
        };

        let token = connection.token();
        if connection.send(poller, response).is_err() {
            let _ = self.connections.remove(&token);
            return Ok(false);
        }

        Ok(true)
    }

    /// Handles an `mio` event.
    pub fn handle_event(&mut self, poller: &mut Poll, event: &Event) -> std::io::Result<()> {
        let token = event.token();
        if token == self.token_min {
            self.handle_listener_event(poller)?;
            return Ok(());
        }

        let Some(connection) = self.connections.get_mut(&token) else {
            return Ok(());
        };

        let mut closed = false;
        connection.handle_event(poller, event, |c, poller| {
            match c.stream_mut().read_value::<REQ>() {
                Err(e) if e.is_io() => {
                    c.close(poller);
                    closed = true;
                    Ok(())
                }
                Err(e) => {
                    let line = c
                        .stream_mut()
                        .read_buf()
                        .splitn(2, |b| *b == b'\n')
                        .next()
                        .unwrap_or(b"");
                    let response =
                        if let Ok(request) = serde_json::from_slice::<RequestObject>(line) {
                            ResponseObject::Err {
                                jsonrpc: jsonlrpc::JsonRpcVersion::V2,
                                error: ErrorObject {
                                    code: ErrorCode::INVALID_PARAMS,
                                    message: e.to_string(),
                                    data: None,
                                },
                                id: request.id,
                            }
                        } else if serde_json::from_slice::<serde_json::Value>(line).is_ok() {
                            ResponseObject::Err {
                                jsonrpc: jsonlrpc::JsonRpcVersion::V2,
                                error: ErrorObject {
                                    code: ErrorCode::INVALID_REQUEST,
                                    message: e.to_string(),
                                    data: None,
                                },
                                id: None,
                            }
                        } else {
                            ResponseObject::Err {
                                jsonrpc: jsonlrpc::JsonRpcVersion::V2,
                                error: ErrorObject {
                                    code: ErrorCode::PARSE_ERROR,
                                    message: e.to_string(),
                                    data: None,
                                },
                                id: None,
                            }
                        };
                    let _ = c.send(poller, &response);
                    c.close(poller);
                    closed = true;
                    Ok(())
                }
                Ok(request) => {
                    self.requests.push_back((From { token }, request));
                    Ok(())
                }
            }
        })?;

        if closed {
            let _ = self.connections.remove(&token);
        }
        Ok(())
    }

    /// Returns client connections.
    pub fn connections(&self) -> impl '_ + Iterator<Item = &Connection> {
        self.connections.values()
    }

    fn handle_listener_event(&mut self, poller: &mut Poll) -> std::io::Result<()> {
        loop {
            match self.listener.accept() {
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) => return Err(e),
                Ok((stream, _addr)) => {
                    let Some(connection) = self.handle_accepted(poller, stream) else {
                        continue;
                    };
                    self.connections.insert(connection.token(), connection);
                }
            }
        }
        Ok(())
    }

    fn handle_accepted(&mut self, poller: &mut Poll, mut stream: TcpStream) -> Option<Connection> {
        let token = self.next_token()?;
        poller
            .registry()
            .register(&mut stream, token, Interest::READABLE)
            .ok()?;
        let connection = Connection::new(token, stream, ConnectionState::Connected);
        Some(connection)
    }

    fn next_token(&mut self) -> Option<Token> {
        if self.token_max.0 - self.token_min.0 == self.connections.len() {
            return None;
        }

        loop {
            let token = self.next_token;
            if self.next_token == self.token_max {
                self.next_token.0 = self.token_min.0 + 1; // `+1` is to skip the server token
            } else {
                self.next_token.0 += 1;
            }
            if !self.connections.contains_key(&token) {
                return Some(token);
            }
        }
    }
}

/// Sender of an RPC request.
#[derive(Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct From {
    token: Token,
}
