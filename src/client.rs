use std::{
    collections::VecDeque,
    io::ErrorKind,
    net::{Shutdown, SocketAddr},
};

use jsonlrpc::{JsonlStream, ResponseObject};
use mio::{event::Event, net::TcpStream, Interest, Poll, Token};
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
    pub fn connect(
        poller: &mut Poll,
        token: Token,
        server_addr: SocketAddr,
    ) -> std::io::Result<Self> {
        let mut stream = TcpStream::connect(server_addr)?;
        stream.set_nodelay(true)?;
        poller
            .registry()
            .register(&mut stream, token, Interest::READABLE)?;
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
        self.stream
            .write_value_to_buf(message)
            .or_else(|e| self.handle_error(poller, e))?;
        if self.connecting {
            return Ok(());
        }

        let start_writing = self.queued_bytes_len() == 0;
        self.handle_write(poller, start_writing)
    }

    pub fn queued_bytes_len(&self) -> usize {
        self.stream.write_buf().len()
    }

    pub fn try_recv(&mut self) -> Option<ResponseObject> {
        self.responses.pop_front()
    }

    pub fn handle_event(&mut self, poller: &mut Poll, event: &Event) -> serde_json::Result<()> {
        debug_assert_eq!(self.token, event.token());

        if self.connecting {
            self.handle_connect(poller)?;
            if self.connecting {
                return Ok(());
            }
        }
        if event.is_writable() {
            self.handle_write(poller, false)?;
        }
        if event.is_readable() {
            self.handle_read(poller)?;
        }
        Ok(())
    }

    fn handle_connect(&mut self, poller: &mut Poll) -> serde_json::Result<()> {
        // See: https://docs.rs/mio/1.0.2/mio/net/struct.TcpStream.html#method.connect
        self.stream
            .inner()
            .take_error()
            .map_err(serde_json::Error::io)?;
        match self.stream.inner().peer_addr() {
            Err(e) if e.kind() == ErrorKind::NotConnected => return Ok(()),
            Err(e) => return self.handle_error(poller, serde_json::Error::io(e)),
            Ok(_) => {}
        }

        self.connecting = false;
        self.handle_write(poller, true)?;

        Ok(())
    }

    fn handle_read(&mut self, poller: &mut Poll) -> serde_json::Result<()> {
        match self.stream.read_value() {
            Err(e) => self.handle_error(poller, e),
            Ok(response) => {
                self.responses.push_back(response);
                Ok(())
            }
        }
    }

    fn handle_write(&mut self, poller: &mut Poll, start_writing: bool) -> serde_json::Result<()> {
        let result = match self.stream.flush() {
            Err(e) if e.io_error_kind() == Some(ErrorKind::WouldBlock) => {
                if start_writing {
                    let interests = Interest::READABLE | Interest::WRITABLE;
                    poller
                        .registry()
                        .reregister(self.stream.inner_mut(), self.token, interests)
                        .map_err(serde_json::Error::io)
                } else {
                    Ok(())
                }
            }
            Err(e) => Err(e),
            Ok(_) => {
                if self.queued_bytes_len() == 0 && !start_writing {
                    poller
                        .registry()
                        .reregister(self.stream.inner_mut(), self.token, Interest::READABLE)
                        .map_err(serde_json::Error::io)
                } else {
                    Ok(())
                }
            }
        };
        result.or_else(|e| self.handle_error(poller, e))
    }

    fn handle_error(
        &mut self,
        poller: &mut Poll,
        error: serde_json::Error,
    ) -> serde_json::Result<()> {
        if error.io_error_kind() == Some(ErrorKind::WouldBlock) {
            return Ok(());
        }
        let _ = poller.registry().deregister(self.stream.inner_mut());
        let _ = self.stream.inner().shutdown(Shutdown::Both);
        Err(error)
    }
}

#[derive(Debug)]
pub struct RpcClient {
    server_addr: SocketAddr,
    token: Token,
    connection: Option<RpcClientConnection>,
}

impl RpcClient {
    pub fn new(token: Token, server_addr: SocketAddr) -> Self {
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
                RpcClientConnection::connect(poller, self.token, self.server_addr)
                    .map_err(serde_json::Error::io)?,
            );
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
        self.connection.as_mut().map_or(None, |c| c.try_recv())
    }

    pub fn handle_event(&mut self, poller: &mut Poll, event: &Event) -> serde_json::Result<()> {
        let Some(c) = &mut self.connection else {
            return Ok(());
        };
        c.handle_event(poller, event)
            .map_err(|e| self.handle_error(e))
    }

    fn handle_error(&mut self, error: serde_json::Error) -> serde_json::Error {
        if error.is_io() {
            self.connection = None;
        }
        error
    }
}
