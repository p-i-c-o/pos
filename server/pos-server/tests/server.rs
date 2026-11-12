use clap::Parser;
use pos_server::{DEFAULT_LISTEN, ServerConfig, bind, serve};
use std::{io, net::SocketAddr};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::oneshot,
    time::{Duration, timeout},
};

fn config(addr: &str) -> ServerConfig {
    ServerConfig::try_parse_from(["pos-server", "--listen", addr]).unwrap()
}

async fn http(address: SocketAddr, method: &str, path: &str) -> String {
    let mut socket = tokio::net::TcpStream::connect(address).await.unwrap();
    socket
        .write_all(
            format!("{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )
        .await
        .unwrap();
    let mut response = Vec::new();
    timeout(Duration::from_secs(2), socket.read_to_end(&mut response))
        .await
        .expect("HTTP timed out")
        .unwrap();
    String::from_utf8(response).unwrap()
}

#[test]
fn defaults_and_invalid_input() {
    let default = ServerConfig::try_parse_from(["pos-server"]).unwrap();
    if std::env::var_os("POS_SERVER_LISTEN").is_none() {
        assert_eq!(default.listen.to_string(), DEFAULT_LISTEN);
    }
    assert!(config("127.0.0.1:0").validate().is_ok());
    assert!(config("[::1]:8787").validate().is_ok());
    for addr in [
        "0.0.0.0:8787",
        "[::]:8787",
        "192.168.1.1:8787",
        "8.8.8.8:8787",
        "[::ffff:127.0.0.1]:8787",
    ] {
        assert!(
            config(addr)
                .validate()
                .unwrap_err()
                .to_string()
                .contains("refusing non-loopback")
        );
    }
    assert!(ServerConfig::try_parse_from(["pos-server", "--listen", "bad"]).is_err());
}

#[tokio::test]
async fn unsafe_bind_is_rejected_before_opening_a_listener() {
    let error = bind(&config("0.0.0.0:0")).await.unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
}

#[tokio::test]
async fn occupied_port_is_reported_without_killing_the_other_listener() {
    let listener = bind(&config("127.0.0.1:0")).await.unwrap();
    let existing = listener.local_addr().unwrap();
    let error = bind(&ServerConfig { listen: existing }).await.unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::AddrInUse);
    assert_eq!(listener.local_addr().unwrap(), existing);
}

#[tokio::test]
async fn health_and_unknown_routes_and_shutdown() {
    let listener = bind(&config("127.0.0.1:0")).await.unwrap();
    let address = listener.local_addr().unwrap();
    let (tx, rx) = oneshot::channel::<()>();
    let server = tokio::spawn(serve(listener, async move {
        let _ = rx.await;
    }));

    for route in ["/health", "/api/v1/health"] {
        let result = http(address, "GET", route).await;
        assert!(result.starts_with("HTTP/1.1 200 OK\r\n"), "{result}");
        assert!(
            result.contains("content-type: application/json"),
            "{result}"
        );
        assert!(result.contains("cache-control: no-store"), "{result}");
        assert!(result.contains("\"status\":\"ok\""), "{result}");
        assert!(result.contains("\"api_version\":\"v1\""), "{result}");
        assert!(result.contains("\"version\":\"0.1.0\""), "{result}");
    }

    let unknown = http(address, "GET", "/api/v1/tasks").await;
    assert!(
        unknown.starts_with("HTTP/1.1 404 Not Found\r\n"),
        "{unknown}"
    );
    assert!(unknown.contains("\"code\":\"not_found\""), "{unknown}");
    let disallowed = http(address, "POST", "/api/v1/health").await;
    assert!(disallowed.starts_with("HTTP/1.1 405 Method Not Allowed\r\n"));

    tx.send(()).unwrap();
    timeout(Duration::from_secs(3), server)
        .await
        .expect("shutdown timed out")
        .unwrap()
        .unwrap();
    assert!(tokio::net::TcpStream::connect(address).await.is_err());
}
