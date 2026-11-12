//! Live identity tests run exclusively against disposable localhost pos_ci.
//! Set POS_TEST_DATABASE_URL only in CI, never on a real POS database.

use std::str::FromStr;

use pos_postgres::{IdentityError, PostgresStore};
use sqlx::{PgPool, postgres::PgConnectOptions};

#[tokio::test]
async fn identity_session_and_device_lifecycle() {
    let Ok(url) = std::env::var("POS_TEST_DATABASE_URL") else {
        eprintln!("Auth integration skipped: POS_TEST_DATABASE_URL is unset");
        return;
    };

    let opts = PgConnectOptions::from_str(&url).expect("invalid test URL");
    assert_eq!(opts.get_database(), Some("pos_ci"));
    assert!(matches!(opts.get_host(), "localhost" | "127.0.0.1" | "::1"));

    let store = PostgresStore::open(&url).await.unwrap();
    let raw = PgPool::connect(&url).await.unwrap();
    let suffix: String = sqlx::query_scalar("SELECT replace(gen_random_uuid()::text, '-', '')")
        .fetch_one(&raw)
        .await
        .unwrap();
    let email = format!("Identity{suffix}@Example.test");
    let email2 = format!("Other{suffix}@Example.test");
    let password = "correct horse battery stable 2026";

    let user = store.provision_user(&email, password).await.unwrap();
    assert_eq!(
        store.provision_user(&email.to_lowercase(), password).await,
        Err(IdentityError::AlreadyExists)
    );
    assert_eq!(
        store.provision_user("bad email", password).await,
        Err(IdentityError::InvalidInput)
    );

    assert!(matches!(
        store
            .sign_in(&email, "wrong password", "ThinkPad", None)
            .await,
        Err(IdentityError::InvalidCredentials)
    ));
    assert!(matches!(
        store
            .sign_in("nonexistent@example.test", password, "ThinkPad", None)
            .await,
        Err(IdentityError::InvalidCredentials)
    ));

    let login = store
        .sign_in(&email, password, "ThinkPad", None)
        .await
        .unwrap();
    assert_eq!(login.identity.user_id, user);
    assert_ne!(login.identity.device_id.to_string(), user.to_string());
    assert!(format!("{login:?}").contains("[REDACTED]"));
    assert!(!format!("{login:?}").contains(login.token.expose()));
    assert_eq!(
        store
            .authenticate_token(login.token.expose())
            .await
            .unwrap(),
        Some(login.identity)
    );
    assert!(store.authenticate_token("bad").await.unwrap().is_none());
    assert!(
        store
            .authenticate_token(&"0".repeat(64))
            .await
            .unwrap()
            .is_none()
    );

    let stored: (String, i64) = sqlx::query_as(
        "SELECT password_hash, (SELECT count(*) FROM pos_sessions \
         WHERE user_id = $1) FROM pos_users WHERE id = $1",
    )
    .bind(user)
    .fetch_one(&raw)
    .await
    .unwrap();
    assert!(stored.0.starts_with("$argon2id$"));
    assert!(!stored.0.contains(password));
    assert_eq!(stored.1, 1);

    let second = store
        .sign_in(&email, password, "ThinkPad", Some(login.identity.device_id))
        .await
        .unwrap();
    assert_eq!(second.identity, login.identity);
    assert_ne!(second.token.digest(), login.token.digest());
    assert!(
        store
            .authenticate_token(second.token.expose())
            .await
            .unwrap()
            .is_some()
    );

    let other = store.provision_user(&email2, password).await.unwrap();
    assert_ne!(other, user);
    assert!(matches!(
        store
            .sign_in(
                &email2,
                password,
                "Stolen device",
                Some(login.identity.device_id)
            )
            .await,
        Err(IdentityError::InvalidCredentials)
    ));
    assert!(
        !store
            .revoke_device(other, login.identity.device_id)
            .await
            .unwrap()
    );

    // Expire one session without violating expires_at > created_at.
    let digest = second.token.digest();
    sqlx::query(
        "UPDATE pos_sessions SET created_at = CURRENT_TIMESTAMP - INTERVAL '2 days', \
         expires_at = CURRENT_TIMESTAMP - INTERVAL '1 day' WHERE token_hash = $1",
    )
    .bind(digest.as_slice())
    .execute(&raw)
    .await
    .unwrap();
    assert!(
        store
            .authenticate_token(second.token.expose())
            .await
            .unwrap()
            .is_none()
    );

    assert!(store.revoke_token(login.token.expose()).await.unwrap());
    assert!(!store.revoke_token(login.token.expose()).await.unwrap());
    assert!(
        store
            .authenticate_token(login.token.expose())
            .await
            .unwrap()
            .is_none()
    );

    let third = store
        .sign_in(&email, password, "ThinkPad", Some(login.identity.device_id))
        .await
        .unwrap();
    assert_eq!(
        store
            .authenticate_token(third.token.expose())
            .await
            .unwrap(),
        Some(login.identity)
    );
    assert!(
        store
            .revoke_device(user, login.identity.device_id)
            .await
            .unwrap()
    );
    assert!(
        !store
            .revoke_device(user, login.identity.device_id)
            .await
            .unwrap()
    );
    assert!(
        store
            .authenticate_token(third.token.expose())
            .await
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        store
            .sign_in(&email, password, "ThinkPad", Some(login.identity.device_id))
            .await,
        Err(IdentityError::InvalidCredentials)
    ));

    let fresh = store
        .sign_in(&email, password, "A different device", None)
        .await
        .unwrap();
    assert_ne!(fresh.identity.device_id, login.identity.device_id);
    assert!(store.disable_user(user).await.unwrap());
    assert!(!store.disable_user(user).await.unwrap());
    assert!(
        store
            .authenticate_token(fresh.token.expose())
            .await
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        store.sign_in(&email, password, "Another", None).await,
        Err(IdentityError::InvalidCredentials)
    ));

    raw.close().await;
    store.close().await;
}
