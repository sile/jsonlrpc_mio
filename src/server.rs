use std::{
    collections::{HashMap, VecDeque},
    io::ErrorKind,
    marker::PhantomData,
    net::SocketAddr,
};

use jsonlrpc::JsonlStream;
use mio::{
    event::Event,
    net::{TcpListener, TcpStream},
    Interest, Poll, Token,
};
use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub struct RpcServer<REQ> {
    listen_addr: SocketAddr,
    listener: TcpListener,
    token_start: Token,
    token_end: Token,
    next_token: Token,
    connections: HashMap<Token, Connection>,
    requests: VecDeque<(From, REQ)>,
    _request: PhantomData<REQ>,
}

impl<REQ> RpcServer<REQ>
where
    REQ: for<'de> Deserialize<'de>,
{
    pub fn start(
        poller: &mut Poll,
        listen_addr: SocketAddr,
        token_start: Token,
        token_end: Token,
    ) -> std::io::Result<Self> {
        if !(token_start < token_end) {
            return Err(std::io::Error::new(
                ErrorKind::InvalidInput,
                "Empty token range",
            ));
        }

        let mut listener = TcpListener::bind(listen_addr)?;
        let listen_addr = listener.local_addr()?;
        poller
            .registry()
            .register(&mut listener, token_start, Interest::READABLE)?;
        Ok(Self {
            listen_addr,
            listener,
            token_start,
            token_end,
            next_token: Token(token_start.0 + 1),
            connections: HashMap::new(),
            requests: VecDeque::new(),
            _request: PhantomData,
        })
    }

    pub fn listen_addr(&self) -> SocketAddr {
        self.listen_addr
    }

    // TODO: connections()

    pub fn handle_event(&mut self, poller: &mut Poll, event: &Event) -> std::io::Result<bool> {
        let token = event.token();
        if token == self.token_start {
            self.handle_listener_event(poller)?;
            Ok(true)
        } else if let Some(connection) = self.connections.get_mut(&token) {
            connection.handle_event(poller, event)?;
            Ok(true)
        } else {
            Ok(false)
        }
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
                    self.connections.insert(connection.token, connection);
                }
            }
        }
        Ok(())
    }

    fn handle_accepted(&mut self, poller: &mut Poll, mut stream: TcpStream) -> Option<Connection> {
        stream.set_nodelay(true).ok()?;
        let token = self.next_token()?;
        poller
            .registry()
            .register(&mut stream, token, Interest::READABLE)
            .ok()?;
        Some(Connection {
            token,
            stream: JsonlStream::new(stream),
        })
    }

    fn next_token(&mut self) -> Option<Token> {
        if self.token_end.0 - self.token_start.0 == self.connections.len() + 1 {
            return None;
        }

        loop {
            let token = self.next_token;
            self.next_token.0 += 1;
            if self.next_token == self.token_end {
                self.next_token.0 = self.token_start.0 + 1;
            }
            if !self.connections.contains_key(&token) {
                return Some(token);
            }
        }
    }

    pub fn reply<T: Serialize>(&mut self, from: From, response: &T) -> serde_json::Result<()> {
        todo!()
    }
}

#[derive(Debug)]
pub struct From {
    token: Token,
}

// TODO(?): RpcServerConnection
#[derive(Debug)]
struct Connection {
    token: Token,
    stream: JsonlStream<TcpStream>,
}

impl Connection {
    fn handle_event(&mut self, poller: &mut Poll, event: &Event) -> std::io::Result<()> {
        todo!()
    }
}
