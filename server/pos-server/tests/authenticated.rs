//! Live authenticated HTTP tests. Requires an isolated loopback PostgreSQL
//! database named pos_ci. Never run this against a real POS database.

use std::{net::SocketAddr, str::FromStr, time::SystemTime};

use pos_permissions::{Role, WorkspaceKind};
use pos_postgres::PostgresStore;
use pos_server::{ServerConfig, bind, serve_with_store};
use sqlx::postgres::PgConnectOptions;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::oneshot,
    time::{Duration, timeout},
};

async fn request(
    address: SocketAddr,
    method: &str,
    path: &str,
    authorization_headers: &[&str],
) -> String {
    let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
    let auth = authorization_headers
        .iter()
        .map(|value| format!("Authorization: {value}\r\n"))
        .collect::<String>();
    let request =
        format!("{method} {path} HTTP/1.1\r\nHost: localhost\r\n{auth}Connection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut bytes = Vec::new();
    timeout(Duration::from_secs(5), stream.read_to_end(&mut bytes))
        .await
        .expect("server timed out")
        .unwrap();
    String::from_utf8(bytes).unwrap()
}

fn check(response: &str, status: &str) {
    assert!(response.starts_with(status), "{response}");
    assert!(response.contains("cache-control: no-store"), "{response}");
}

#[tokio::test]
async fn authenticated_read_only_workspaces_enforce_user_isolation() {
    let Ok(url) = std::env::var("POS_TEST_DATABASE_URL") else {
        eprintln!("Authenticated API test skipped: POS_TEST_DATABASE_URL not set");
        return;
    };
    let options = PgConnectOptions::from_str(&url).expect("invalid test URL");
    assert_eq!(
        options.get_database(),
        Some("pos_ci"),
        "Only disposable pos_ci is allowed"
    );
    assert!(matches!(
        options.get_host(),
        "127.0.0.1" | "localhost" | "::1"
    ));

    let store = PostgresStore::open(&url).await.unwrap();
    let suffix = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let password = "test-only long passphrase 2026";
    let alice = store
        .provision_user(&format!("alice{suffix}@example.test"), password)
        .await
        .unwrap();
    let bob = store
        .provision_user(&format!("bob{suffix}@example.test"), password)
        .await
        .unwrap();

    let personal = store
        .create_workspace(alice, WorkspaceKind::Personal, "Alice personal")
        .await
        .unwrap()
        .unwrap();
    let shared = store
        .create_workspace(alice, WorkspaceKind::Shared, "Private project")
        .await
        .unwrap()
        .unwrap();
    let bob_personal = store
        .create_workspace(bob, WorkspaceKind::Personal, "Bob personal")
        .await
        .unwrap()
        .unwrap();

    // No automatic sharing; not even an authenticated user can see others.
    assert!(
        !store
            .grant_membership(alice, personal, bob, Role::Viewer)
            .await
            .unwrap()
    );
    assert!(
        !store
            .grant_membership(bob, shared, bob, Role::Viewer)
            .await
            .unwrap()
    );
    assert!(
        !store
            .grant_membership(alice, shared, bob, Role::Owner)
            .await
            .unwrap()
    );

    let alice_login = store
        .sign_in(
            &format!("alice{suffix}@example.test"),
            password,
            "Laptop",
            None,
        )
        .await
        .unwrap();
    let bob_login = store
        .sign_in(
            &format!("bob{suffix}@example.test"),
            password,
            "Laptop",
            None,
        )
        .await
        .unwrap();

    let listener = bind(&ServerConfig {
        listen: "127.0.0.1:0".parse().unwrap(),
    })
    .await
    .unwrap();
    let address = listener.local_addr().unwrap();
    let (tx, rx) = oneshot::channel::<()>();
    let running_store = store.clone();
    let server = tokio::spawn(serve_with_store(
        listener,
        async move {
            let _ = rx.await;
        },
        Some(running_store),
    ));

    let alice_header = format!("Bearer {}", alice_login.token.expose());
    let bob_header = format!("Bearer {}", bob_login.token.expose());

    let response = request(address, "GET", "/api/v1/auth/me", &[]).await;
    check(&response, "HTTP/1.1 401 Unauthorized\r\n");
    let response = request(address, "GET", "/api/v1/auth/me", &["Bearer bad"]).await;
    check(&response, "HTTP/1.1 401 Unauthorized\r\n");
    let response = request(
        address,
        "GET",
        "/api/v1/auth/me",
        &[&alice_header, &bob_header],
    )
    .await;
    check(&response, "HTTP/1.1 401 Unauthorized\r\n");
    let response = request(address, "GET", "/api/v1/auth/me", &[&alice_header]).await;
    check(&response, "HTTP/1.1 200 OK\r\n");
    assert!(response.contains(&alice.to_string()));
    assert!(response.contains(&alice_login.identity.device_id.to_string()));
    assert!(!response.contains(password));
    assert!(!response.contains(alice_login.token.expose()));

    let alice_list = request(address, "GET", "/api/v1/workspaces", &[&alice_header]).await;
    check(&alice_list, "HTTP/1.1 200 OK\r\n");
    assert!(alice_list.contains("Alice personal"));
    assert!(alice_list.contains("Private project"));
    assert!(!alice_list.contains("Bob personal"));
    assert!(alice_list.contains("\"role\":\"owner\""));
    assert!(alice_list.contains("\"can_manage_members\":true"));

    let bob_list = request(address, "GET", "/api/v1/workspaces", &[&bob_header]).await;
    check(&bob_list, "HTTP/1.1 200 OK\r\n");
    assert!(bob_list.contains("Bob personal"));
    assert!(!bob_list.contains("Private project"));
    assert!(!bob_list.contains("Alice personal"));

    let personal_path = format!("/api/v1/workspaces/{personal}");
    let shared_path = format!("/api/v1/workspaces/{shared}");
    let bob_path = format!("/api/v1/workspaces/{bob_personal}");
    let response = request(address, "GET", &personal_path, &[&bob_header]).await;
    check(&response, "HTTP/1.1 404 Not Found\r\n");
    let response = request(address, "GET", &shared_path, &[&bob_header]).await;
    check(&response, "HTTP/1.1 404 Not Found\r\n");
    let response = request(address, "GET", &bob_path, &[&alice_header]).await;
    check(&response, "HTTP/1.1 404 Not Found\r\n");

    assert!(
        store
            .grant_membership(alice, shared, bob, Role::Viewer)
            .await
            .unwrap()
    );
    let response = request(address, "GET", &shared_path, &[&bob_header]).await;
    check(&response, "HTTP/1.1 200 OK\r\n");
    assert!(response.contains("\"role\":\"viewer\""));
    assert!(response.contains("\"can_read\":true"));
    assert!(response.contains("\"can_write\":false"));
    assert!(response.contains("\"can_manage_members\":false"));
    let response = request(address, "GET", &personal_path, &[&bob_header]).await;
    check(&response, "HTTP/1.1 404 Not Found\r\n");

    assert!(
        store
            .grant_membership(alice, shared, bob, Role::Editor)
            .await
            .unwrap()
    );
    let response = request(address, "GET", &shared_path, &[&bob_header]).await;
    check(&response, "HTTP/1.1 200 OK\r\n");
    assert!(response.contains("\"role\":\"editor\""));
    assert!(response.contains("\"can_write\":true"));
    assert!(response.contains("\"can_manage_members\":false"));
    assert!(store.remove_membership(alice, shared, bob).await.unwrap());
    let response = request(address, "GET", &shared_path, &[&bob_header]).await;
    check(&response, "HTTP/1.1 404 Not Found\r\n");

    // Both unauthenticated and authenticated write operations remain absent.
    let response = request(address, "POST", "/api/v1/workspaces", &[&alice_header]).await;
    assert!(response.starts_with("HTTP/1.1 405 Method Not Allowed\r\n"));
    let response = request(address, "GET", "/api/v1/tasks", &[&alice_header]).await;
    assert!(response.starts_with("HTTP/1.1 404 Not Found\r\n"));

    assert!(store.revoke_token(bob_login.token.expose()).await.unwrap());
    let response = request(address, "GET", "/api/v1/auth/me", &[&bob_header]).await;
    check(&response, "HTTP/1.1 401 Unauthorized\r\n");

    assert!(store.disable_user(alice).await.unwrap());
    let response = request(address, "GET", "/api/v1/workspaces", &[&alice_header]).await;
    check(&response, "HTTP/1.1 401 Unauthorized\r\n");

    tx.send(()).unwrap();
    timeout(Duration::from_secs(4), server)
        .await
        .expect("server shutdown hung")
        .unwrap()
        .unwrap();
    store.close().await;
}
