//! End-to-end loopback HTTP account onboarding against isolated pos_ci.
//! Skips locally without POS_TEST_DATABASE_URL; CI always supplies it.

use std::{net::SocketAddr, str::FromStr, time::SystemTime};

use pos_postgres::PostgresStore;
use pos_server::{ServerConfig, bind, serve_with_store};
use serde_json::{Value, json};
use sqlx::postgres::PgConnectOptions;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::oneshot,
    time::{Duration, timeout},
};
use uuid::Uuid;

async fn request(
    addr: SocketAddr,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: Option<&Value>,
) -> (u16, Value, String) {
    let payload = body.map(ToString::to_string).unwrap_or_default();
    let header = token
        .map(|t| format!("Authorization: Bearer {t}\r\n"))
        .unwrap_or_default();
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\
         Content-Type: application/json\r\n{header}Content-Length: {}\r\n\r\n{payload}",
        payload.len()
    );
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut output = Vec::new();
    timeout(Duration::from_secs(5), stream.read_to_end(&mut output))
        .await
        .expect("HTTP response timeout")
        .unwrap();
    let raw = String::from_utf8(output).unwrap();
    let (head, data) = raw
        .split_once("\r\n\r\n")
        .expect("no HTTP header/body separator");
    let status: u16 = head.split_whitespace().nth(1).unwrap().parse().unwrap();
    if status != 405 {
        assert!(
            head.contains("cache-control: no-store"),
            "missing no-store in {head}"
        );
    }
    let json = serde_json::from_str(data).unwrap_or(Value::Null);
    (status, json, head.to_owned())
}

#[tokio::test]
async fn create_user_login_device_reuse_and_revoke_session() {
    let Ok(url) = std::env::var("POS_TEST_DATABASE_URL") else {
        eprintln!("Onboarding API test skipped; POS_TEST_DATABASE_URL unset");
        return;
    };
    let options = PgConnectOptions::from_str(&url).expect("bad test database URL");
    assert_eq!(options.get_database(), Some("pos_ci"));
    assert!(matches!(
        options.get_host(),
        "localhost" | "127.0.0.1" | "::1"
    ));

    let store = PostgresStore::open(&url).await.unwrap();
    let suffix = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let email = format!("onboarding-{suffix}@example.test");
    let password = "CI disposable long passphrase 2026";
    let (user, personal) = store
        .provision_user_with_personal_workspace(&email, password)
        .await
        .unwrap();
    let workspaces = store.list_user_workspaces(user).await.unwrap();
    assert_eq!(workspaces.len(), 1);
    assert_eq!(workspaces[0].id, personal);
    assert_eq!(workspaces[0].name, "Personal");
    assert_eq!(workspaces[0].kind.as_str(), "personal");
    assert!(
        store
            .provision_user_with_personal_workspace(&email, password)
            .await
            .is_err()
    );

    let listener = bind(&ServerConfig {
        listen: "127.0.0.1:0".parse().unwrap(),
    })
    .await
    .unwrap();
    let address = listener.local_addr().unwrap();
    let (stop, stopped) = oneshot::channel::<()>();
    let server = tokio::spawn(serve_with_store(
        listener,
        async move {
            let _ = stopped.await;
        },
        Some(store.clone()),
    ));

    let valid = json!({
        "email": email, "password": password, "device_label": "ThinkPad",
        "existing_device": null
    });
    let wrong = json!({
        "email": email, "password": "incorrect password",
        "device_label": "ThinkPad", "existing_device": null
    });
    let (code, bad, _) = request(address, "POST", "/api/v1/auth/login", None, Some(&wrong)).await;
    assert_eq!(code, 401);
    assert!(!bad.to_string().contains(&email));
    assert!(!bad.to_string().contains("incorrect password"));
    let (code, response, _) =
        request(address, "POST", "/api/v1/auth/login", None, Some(&valid)).await;
    assert_eq!(code, 200);
    let secret = response["token"].as_str().unwrap().to_owned();
    assert_eq!(secret.len(), 64);
    assert_eq!(response["user_id"].as_str().unwrap(), user.to_string());
    assert_eq!(response["expires_in_seconds"], 24 * 3600);
    let device = Uuid::parse_str(response["device_id"].as_str().unwrap()).unwrap();

    // Authentication and workspace discovery work with the newly issued token.
    let (code, identity, _) = request(address, "GET", "/api/v1/auth/me", Some(&secret), None).await;
    assert_eq!(code, 200);
    assert_eq!(identity["device_id"].as_str().unwrap(), device.to_string());
    let (code, spaces, _) =
        request(address, "GET", "/api/v1/workspaces", Some(&secret), None).await;
    assert_eq!(code, 200);
    assert_eq!(spaces[0]["id"].as_str().unwrap(), personal.to_string());

    // Password-only re-auth on the same device mints a distinct 24-hour token.
    let mut reuse = valid.clone();
    reuse["existing_device"] = json!(device);
    let (code, reused, _) =
        request(address, "POST", "/api/v1/auth/login", None, Some(&reuse)).await;
    assert_eq!(code, 200);
    assert_eq!(reused["device_id"].as_str().unwrap(), device.to_string());
    assert_ne!(reused["token"].as_str().unwrap(), secret);
    let fresh = reused["token"].as_str().unwrap();

    // Revocation of one session does not revoke other sessions or the device.
    let (code, _, _) = request(address, "POST", "/api/v1/auth/logout", Some(&secret), None).await;
    assert_eq!(code, 204);
    let (code, _, _) = request(address, "GET", "/api/v1/auth/me", Some(&secret), None).await;
    assert_eq!(code, 401);
    let (code, _, _) = request(address, "GET", "/api/v1/auth/me", Some(fresh), None).await;
    assert_eq!(code, 200);
    let (code, _, _) = request(address, "POST", "/api/v1/auth/logout", Some(&secret), None).await;
    assert_eq!(code, 401);

    // A request cannot attach another user's device ID.
    let second = format!("second-{suffix}@example.test");
    let (other, _) = store
        .provision_user_with_personal_workspace(&second, password)
        .await
        .unwrap();
    assert_ne!(other, user);
    let foreign = json!({
        "email": second, "password": password, "device_label": "Invalid",
        "existing_device": device
    });
    let (code, _, _) = request(address, "POST", "/api/v1/auth/login", None, Some(&foreign)).await;
    assert_eq!(code, 401);

    // Global login budget stops brute-force after 8 total attempts per minute.
    // We have already consumed four attempts (wrong, correct, reused, foreign).
    for _ in 0..4 {
        let (code, _, _) = request(address, "POST", "/api/v1/auth/login", None, Some(&wrong)).await;
        assert_eq!(code, 401);
    }
    let (code, _, _) = request(address, "POST", "/api/v1/auth/login", None, Some(&valid)).await;
    assert_eq!(code, 429);

    let _ = stop.send(());
    timeout(Duration::from_secs(5), server)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    store.close().await;
}
