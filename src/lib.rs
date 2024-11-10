mod client;
mod connection;
mod server;

pub use self::client::RpcClient;
pub use self::connection::{Connection, ConnectionState};
pub use self::server::RpcServer;

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use jsonlrpc::{RequestId, RequestObject, ResponseObject};
    use mio::{Events, Poll, Token};
    use orfail::OrFail;

    use super::*;

    const SERVER_TOKEN_START: Token = Token(0);
    const SERVER_TOKEN_END: Token = Token(100);
    const CLIENT_TOKEN: Token = Token(101);

    #[test]
    fn base_server_and_client() -> orfail::Result<()> {
        let mut poller = Poll::new().or_fail()?;
        let mut events = Events::with_capacity(1024);

        let mut server: RpcServer = RpcServer::start(
            &mut poller,
            SocketAddr::from(([127, 0, 0, 1], 0)),
            SERVER_TOKEN_START,
            SERVER_TOKEN_END,
        )
        .or_fail()?;
        let mut client = RpcClient::new(CLIENT_TOKEN, server.listen_addr());

        let request_id = RequestId::Number(123);
        let request = RequestObject {
            jsonrpc: jsonlrpc::JsonRpcVersion::V2,
            method: "ping".to_owned(),
            params: None,
            id: Some(request_id.clone()),
        };
        client.send(&mut poller, &request).or_fail()?;

        'root: loop {
            poller.poll(&mut events, None).or_fail()?;
            for event in events.iter() {
                if server.handle_event(&mut poller, &event).or_fail()? {
                    if let Some((from, request)) = server.try_recv() {
                        assert_eq!(request.method, "ping");
                        let response = ResponseObject::Ok {
                            jsonrpc: jsonlrpc::JsonRpcVersion::V2,
                            result: serde_json::json! { "pong" },
                            id: request_id.clone(),
                        };
                        server.reply(&mut poller, from, &response).or_fail()?;
                    }
                    continue;
                }

                client.handle_event(&mut poller, &event).or_fail()?;
                if let Some(response) = client.try_recv() {
                    assert_eq!(response.id(), Some(&request_id));
                    let Ok(value) = response.into_std_result() else {
                        panic!();
                    };
                    assert_eq!(value, serde_json::json! { "pong" });
                    break 'root;
                }
            }
        }

        Ok(())
    }
}
