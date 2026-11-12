//! Real HTTP CRUD + PostgreSQL access tests on a disposable pos_ci database.
//! No test may connect to a remote or non-pos_ci PostgreSQL database.

use std::{net::SocketAddr, str::FromStr, time::SystemTime};

use pos_permissions::{Role, WorkspaceKind};
use pos_postgres::{PostgresStore, RecordError};
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
) -> (u16, Value) {
    let content = body.map(ToString::to_string).unwrap_or_default();
    let auth = token
        .map(|token| format!("Authorization: Bearer {token}\r\n"))
        .unwrap_or_default();
    let message = format!(
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\n\
         Connection: close\r\n{auth}Content-Type: application/json\r\n\
         Content-Length: {}\r\n\r\n{content}",
        content.len()
    );
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    stream.write_all(message.as_bytes()).await.unwrap();
    let mut output = Vec::new();
    timeout(Duration::from_secs(5), stream.read_to_end(&mut output))
        .await
        .expect("HTTP timed out")
        .unwrap();
    let raw = String::from_utf8(output).unwrap();
    let (head, data) = raw.split_once("\r\n\r\n").expect("missing response body");
    let status: u16 = head
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    if status != 405 {
        assert!(head.contains("cache-control: no-store"), "{head}");
    }
    let parsed = serde_json::from_str(data).unwrap_or(Value::Null);
    (status, parsed)
}

fn uuid(v: &Value) -> Uuid {
    Uuid::parse_str(v.as_str().unwrap()).unwrap()
}

#[tokio::test]
async fn workspace_records_are_scoped_versioned_and_validated() {
    let Ok(url) = std::env::var("POS_TEST_DATABASE_URL") else {
        eprintln!("POS data API test skipped: POS_TEST_DATABASE_URL not set");
        return;
    };
    let options = PgConnectOptions::from_str(&url).expect("invalid test URL");
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
    let pass = "disposable CI-only passphrase 2026";
    let alice_email = format!("records-a{suffix}@example.test");
    let bob_email = format!("records-b{suffix}@example.test");
    let alice = store.provision_user(&alice_email, pass).await.unwrap();
    let bob = store.provision_user(&bob_email, pass).await.unwrap();
    let private = store
        .create_workspace(alice, WorkspaceKind::Personal, "Alice's private")
        .await
        .unwrap()
        .unwrap();
    let shared = store
        .create_workspace(alice, WorkspaceKind::Shared, "Shared workspace")
        .await
        .unwrap()
        .unwrap();
    let other = store
        .create_workspace(bob, WorkspaceKind::Personal, "Bob's private")
        .await
        .unwrap()
        .unwrap();
    let a_login = store.sign_in(&alice_email, pass, "A", None).await.unwrap();
    let b_login = store.sign_in(&bob_email, pass, "B", None).await.unwrap();

    let listener = bind(&ServerConfig {
        listen: "127.0.0.1:0".parse().unwrap(),
    })
    .await
    .unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = oneshot::channel::<()>();
    let server = tokio::spawn(serve_with_store(
        listener,
        async move {
            let _ = rx.await;
        },
        Some(store.clone()),
    ));

    let a = a_login.token.expose();
    let b = b_login.token.expose();
    let a_projects = format!("/api/v1/workspaces/{private}/projects");
    let a_tasks = format!("/api/v1/workspaces/{private}/tasks");
    let b_projects = format!("/api/v1/workspaces/{other}/projects");
    let shared_projects = format!("/api/v1/workspaces/{shared}/projects");
    let shared_tasks = format!("/api/v1/workspaces/{shared}/tasks");

    // Identity is mandatory even for listing an empty collection.
    assert_eq!(request(addr, "GET", &a_projects, None, None).await.0, 401);
    assert_eq!(
        request(addr, "GET", &a_projects, Some(b), None).await.0,
        404
    );
    assert_eq!(request(addr, "GET", &a_tasks, Some(b), None).await.0, 404);
    assert_eq!(request(addr, "GET", &a_tasks, Some(a), None).await.0, 200);

    // Create and read a project. Slugs are workspace-local.
    let (status, p) = request(
        addr,
        "POST",
        &a_projects,
        Some(a),
        Some(&json!({"name":"University tasks"})),
    )
    .await;
    assert_eq!(status, 201, "{p}");
    assert_eq!(p["slug"], "university-tasks");
    assert_eq!(p["version"], 1);
    let project = uuid(&p["id"]);
    let p_path = format!("{a_projects}/{project}");
    assert_eq!(request(addr, "GET", &p_path, Some(b), None).await.0, 404);
    assert_eq!(request(addr, "GET", &p_path, Some(a), None).await.0, 200);
    assert_eq!(
        request(
            addr,
            "POST",
            &a_projects,
            Some(a),
            Some(&json!({"name":"University tasks"}))
        )
        .await
        .0,
        409
    );
    assert_eq!(
        request(
            addr,
            "POST",
            &a_projects,
            Some(b),
            Some(&json!({"name":"Never"}))
        )
        .await
        .0,
        404
    );
    assert_eq!(
        request(
            addr,
            "POST",
            &a_projects,
            Some(a),
            Some(&json!({"name":"   "}))
        )
        .await
        .0,
        400
    );
    let (status, updated) = request(
        addr,
        "PATCH",
        &p_path,
        Some(a),
        Some(&json!({"expected_version":1, "name":"Coursework"})),
    )
    .await;
    assert_eq!(status, 200, "{updated}");
    assert_eq!(updated["version"], 2);
    assert_eq!(updated["slug"], "university-tasks");
    assert_eq!(
        request(
            addr,
            "PATCH",
            &p_path,
            Some(a),
            Some(&json!({"expected_version":1, "archived":true}))
        )
        .await
        .0,
        409
    );

    // Task validation, priority/status/due dates, and same-workspace project FK.
    let (status, task) = request(
        addr,
        "POST",
        &a_tasks,
        Some(a),
        Some(&json!({
            "title":"Complete report",
            "description":"Important",
            "priority":"high",
            "due_date":"2028-02-29",
            "project_id":project.to_string()
        })),
    )
    .await;
    assert_eq!(status, 201, "{task}");
    let task_id = uuid(&task["id"]);
    let t_path = format!("{a_tasks}/{task_id}");
    assert_eq!(task["priority"], "high");
    assert_eq!(task["status"], "todo");
    assert_eq!(task["due_date"], "2028-02-29");
    assert_eq!(task["completed"], false);
    assert_eq!(task["version"], 1);
    assert_eq!(request(addr, "GET", &t_path, Some(b), None).await.0, 404);
    assert_eq!(request(addr, "GET", &t_path, Some(a), None).await.0, 200);
    let list = request(addr, "GET", &a_tasks, Some(a), None).await;
    assert_eq!(list.0, 200);
    assert_eq!(list.1.as_array().unwrap().len(), 1);

    for bad in [
        json!({"title":"Invalid date", "due_date":"2027-02-29"}),
        json!({"title":"Invalid priority", "priority":"critical"}),
        json!({"title":"Invalid status", "status":"maybe"}),
        json!({"title":"    "}),
    ] {
        assert_eq!(
            request(addr, "POST", &a_tasks, Some(a), Some(&bad)).await.0,
            400
        );
    }
    assert_eq!(
        request(
            addr,
            "POST",
            &a_tasks,
            Some(b),
            Some(&json!({"title":"B attack"}))
        )
        .await
        .0,
        404
    );

    // Bob's workspace project cannot be assigned into Alice's task, even
    // when Alice supplies the UUID and has editor rights elsewhere.
    let (status, other_project) = request(
        addr,
        "POST",
        &b_projects,
        Some(b),
        Some(&json!({"name":"Private B"})),
    )
    .await;
    assert_eq!(status, 201);
    assert_eq!(
        request(
            addr,
            "POST",
            &a_tasks,
            Some(a),
            Some(&json!({"title":"Foreign link", "project_id":other_project["id"]}))
        )
        .await
        .0,
        404
    );

    // Version changes atomically. Explicit null clears nullable fields.
    let (status, completed) = request(
        addr,
        "PATCH",
        &t_path,
        Some(a),
        Some(&json!({
            "expected_version":1, "status":"completed",
            "due_date":null, "description":null, "project_id":null
        })),
    )
    .await;
    assert_eq!(status, 200, "{completed}");
    assert_eq!(completed["version"], 2);
    assert_eq!(completed["completed"], true);
    assert_eq!(completed["status"], "completed");
    assert!(completed["due_date"].is_null());
    assert!(completed["description"].is_null());
    assert!(completed["project_id"].is_null());
    assert_eq!(
        request(
            addr,
            "PATCH",
            &t_path,
            Some(a),
            Some(&json!({"expected_version":1,"status":"todo"}))
        )
        .await
        .0,
        409
    );
    assert_eq!(
        request(
            addr,
            "PATCH",
            &t_path,
            Some(a),
            Some(&json!({"expected_version":2}))
        )
        .await
        .0,
        400
    );
    assert_eq!(
        request(
            addr,
            "PATCH",
            &t_path,
            Some(b),
            Some(&json!({"expected_version":2,"status":"todo"}))
        )
        .await
        .0,
        404
    );

    // Reader can read but never write; Editor can write but cannot gain
    // access to private workspaces. Revocation takes effect immediately.
    assert!(
        store
            .grant_membership(alice, shared, bob, Role::Viewer)
            .await
            .unwrap()
    );
    let (status, shared_project) = request(
        addr,
        "POST",
        &shared_projects,
        Some(a),
        Some(&json!({"name":"Shared project"})),
    )
    .await;
    assert_eq!(status, 201);
    let s_project = uuid(&shared_project["id"]);
    assert_eq!(
        request(addr, "GET", &shared_projects, Some(b), None)
            .await
            .0,
        200
    );
    assert_eq!(
        request(
            addr,
            "POST",
            &shared_tasks,
            Some(b),
            Some(&json!({"title":"Not allowed"}))
        )
        .await
        .0,
        404
    );
    assert_eq!(
        request(
            addr,
            "PATCH",
            &format!("{shared_projects}/{s_project}"),
            Some(b),
            Some(&json!({"expected_version":1,"name":"Nope"}))
        )
        .await
        .0,
        404
    );
    assert!(
        store
            .grant_membership(alice, shared, bob, Role::Editor)
            .await
            .unwrap()
    );
    let (status, editor_task) = request(
        addr,
        "POST",
        &shared_tasks,
        Some(b),
        Some(&json!({"title":"Contribute", "project_id":s_project.to_string()})),
    )
    .await;
    assert_eq!(status, 201, "{editor_task}");
    assert_eq!(
        request(addr, "GET", &a_projects, Some(b), None).await.0,
        404
    );
    assert!(store.remove_membership(alice, shared, bob).await.unwrap());
    assert_eq!(
        request(addr, "GET", &shared_tasks, Some(b), None).await.0,
        404
    );
    assert_eq!(
        request(
            addr,
            "PATCH",
            &format!("{shared_tasks}/{}", editor_task["id"].as_str().unwrap()),
            Some(b),
            Some(&json!({"expected_version":1,"status":"completed"}))
        )
        .await
        .0,
        404
    );
    assert_eq!(
        store
            .update_project_record(bob, private, project, 2, Some("Unauthorized"), None)
            .await
            .err(),
        Some(RecordError::NotFound)
    );

    // Archiving a project is non-destructive. No HTTP hard delete exists.
    let (status, archived) = request(
        addr,
        "PATCH",
        &p_path,
        Some(a),
        Some(&json!({"expected_version":2,"archived":true})),
    )
    .await;
    assert_eq!(status, 200, "{archived}");
    assert_eq!(archived["archived"], true);
    assert_eq!(
        request(
            addr,
            "POST",
            &a_tasks,
            Some(a),
            Some(&json!({"title":"New under archived project", "project_id":project.to_string()}))
        )
        .await
        .0,
        404
    );
    assert_eq!(request(addr, "DELETE", &p_path, Some(a), None).await.0, 405);
    assert_eq!(request(addr, "DELETE", &t_path, Some(a), None).await.0, 405);

    // After token revocation, even read operations fail.
    assert!(store.revoke_token(a).await.unwrap());
    assert_eq!(request(addr, "GET", &a_tasks, Some(a), None).await.0, 401);

    tx.send(()).unwrap();
    timeout(Duration::from_secs(5), server)
        .await
        .expect("shutdown hung")
        .unwrap()
        .unwrap();
    // The server shutdown closes the shared pool; no production data touched.
}
