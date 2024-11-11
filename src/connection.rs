use std::{io::ErrorKind, net::Shutdown};

use jsonlrpc::JsonlStream;
use mio::{event::Event, net::TcpStream, Interest, Poll, Token};
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

    // TODO: crate private
    pub fn handle_event<F>(
        &mut self,
        poller: &mut Poll,
        event: &Event,
        on_read: F,
    ) -> serde_json::Result<()>
    where
        F: FnOnce(&mut Self, &mut Poll) -> serde_json::Result<()>,
    {
        debug_assert_eq!(self.token, event.token());
        self.check_not_closed()?;

        if self.state == ConnectionState::Connecting {
            self.handle_connect(poller)?;
        }
        if event.is_writable() {
            self.handle_write(poller, false)?;
        }
        if event.is_readable() {
            self.handle_read(poller, on_read)?;
        }
        Ok(())
    }

    fn handle_read<F>(&mut self, poller: &mut Poll, on_read: F) -> serde_json::Result<()>
    where
        F: FnOnce(&mut Self, &mut Poll) -> serde_json::Result<()>,
    {
        on_read(self, poller).or_else(|e| self.handle_error(poller, e))
    }

    pub fn send<T: Serialize>(&mut self, poller: &mut Poll, request: &T) -> serde_json::Result<()> {
        self.check_not_closed()?;

        let start_writing = self.queued_bytes_len() == 0;

        self.stream
            .write_value_to_buf(request)
            .or_else(|e| self.handle_error(poller, e))?;
        if self.state == ConnectionState::Connecting {
            return Ok(());
        }

        self.handle_write(poller, start_writing)
    }

    pub(crate) fn stream_mut(&mut self) -> &mut JsonlStream<TcpStream> {
        &mut self.stream
    }

    fn check_not_closed(&mut self) -> serde_json::Result<()> {
        if self.state == ConnectionState::Closed {
            Err(serde_json::Error::io(ErrorKind::NotConnected.into()))
        } else {
            Ok(())
        }
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

        self.state = ConnectionState::Connected;
        self.handle_write(poller, false)?;

        Ok(())
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
