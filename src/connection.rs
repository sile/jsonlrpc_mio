use std::{io::ErrorKind, net::Shutdown};

use jsonlrpc::JsonlStream;
use mio::{net::TcpStream, Interest, Poll, Token};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ConnectionState {
    Connecting,
    Connected,
    Closed,
}

#[derive(Debug)]
pub struct Connection {
    token: Token,
    stream: JsonlStream<TcpStream>,
    state: ConnectionState,
}

impl Connection {
    pub(crate) fn new(token: Token, stream: TcpStream, state: ConnectionState) -> Self {
        let _ = stream.set_nodelay(true);
        Self {
            token,
            stream: JsonlStream::new(stream),
            state,
        }
    }

    pub fn token(&self) -> Token {
        self.token
    }

    pub fn state(&self) -> ConnectionState {
        self.state
    }

    pub fn close(&mut self, poller: &mut Poll) {
        if self.state != ConnectionState::Closed {
            return;
        }

        let _ = poller.registry().deregister(self.stream.inner_mut());
        let _ = self.stream.inner().shutdown(Shutdown::Both);
    }

    pub fn queued_bytes_len(&self) -> usize {
        self.stream.write_buf().len()
    }

    pub fn send<T: Serialize>(&mut self, poller: &mut Poll, request: &T) -> serde_json::Result<()> {
        self.check_closed()?;

        let start_writing = self.queued_bytes_len() == 0;

        self.stream
            .write_value_to_buf(request)
            .or_else(|e| self.handle_error(poller, e))?;
        if self.state == ConnectionState::Connecting {
            return Ok(());
        }

        self.handle_write(poller, start_writing)
    }

    fn check_closed(&mut self) -> serde_json::Result<()> {
        if self.state == ConnectionState::Closed {
            Err(serde_json::Error::io(ErrorKind::NotConnected.into()))
        } else {
            Ok(())
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
        self.close(poller);
        Err(error)
    }
}
