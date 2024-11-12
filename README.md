jsonlrpc_mio
============

[![jsonlrpc_mio](https://img.shields.io/crates/v/jsonlrpc_mio.svg)](https://crates.io/crates/jsonlrpc_mio)
[![Documentation](https://docs.rs/jsonlrpc_mio/badge.svg)](https://docs.rs/jsonlrpc_mio)
[![Actions Status](https://github.com/sile/jsonlrpc_mio/workflows/CI/badge.svg)](https://github.com/sile/jsonlrpc_mio/actions)
![License](https://img.shields.io/crates/l/jsonlrpc_mio)

Non-blocking [`jsonlrpc`] server and client using [`mio`].

[`jsonlrpc`]: https://crates.io/crates/jsonlrpc
[`mio`]: https://crates.io/crates/mio

Examples
--------

```
use std::net::SocketAddr;

use jsonlrpc::{RequestId, RequestObject, ResponseObject};
use jsonlrpc_mio::{RpcClient, RpcServer};
use mio::{Events, Poll, Token};

let mut poller = Poll::new()?;
let mut events = Events::with_capacity(1024);

let mut server: RpcServer = RpcServer::start(
    &mut poller,
    SocketAddr::from(([127, 0, 0, 1], 0)),
    Token(0),
    Token(9),
)?;
let mut client = RpcClient::new(Token(10), server.listen_addr());

let request = RequestObject {
    jsonrpc: jsonlrpc::JsonRpcVersion::V2,
    method: "ping".to_owned(),
    params: None,
    id: Some(RequestId::Number(123)),
};
client.send(&mut poller, &request)?;

loop {
    poller.poll(&mut events, None)?;
    for event in events.iter() {
        server.handle_event(&mut poller, event)?;
        if let Some((from, request)) = server.try_recv() {
            assert_eq!(request.method, "ping");
            let response = ResponseObject::Ok {
                jsonrpc: jsonlrpc::JsonRpcVersion::V2,
                result: serde_json::json! { "pong" },
                id: request.id.unwrap(),
            };
            server.reply(&mut poller, from, &response)?;
        }

        client.handle_event(&mut poller, event)?;
        if let Some(response) = client.try_recv() {
            let value = response.into_std_result().unwrap();
            assert_eq!(value, serde_json::json! { "pong" });
            return Ok(());
        }
    }
}
```
