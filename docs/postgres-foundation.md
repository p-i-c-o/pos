# PostgreSQL server storage foundation (Phase 9a)

**Development milestone only.** This crate is not present in tagged v0.1.0
as a functional server database. It is optionally wired into Axum authenticated identity/workspace
metadata and Tasks/Projects create/read/update endpoints. Phase 9b added
trusted-only user/device credentials, and Phase 9c adds workspace scopes.

## What it provides

- A bounded SQLx PostgreSQL connection pool (maximum 5 connections).
- An explicitly supplied connection URL, with no stored/default credentials.
- A migration runner using versioned SQL files under
  `storage/pos-postgres/migrations/`.
- A stable PostgreSQL server-instance UUID and creation timestamp.
- A database reachability and instance-metadata query.
- Generic, credential-safe error messages.
- An explicit pool-close operation for controlled shutdown.

The first migration creates just one POS-owned table:

`pos_server_instance(singleton, instance_id, created_at)`

It inserts exactly one row for this database. SQLx also maintains
`_sqlx_migrations`. Future migrations must design user/workspace ownership
and authorization boundaries before storing business records. Later migrations
add user/device/session security records and workspace membership metadata.

**Do not use this crate as a generic anonymous data repository.** There are no
Task/Project repository implementations or HTTP business-data routes.

## Connections and migrations

When a trusted caller invokes `PostgresStore::open(database_url)`, it:

1. Validates a PostgreSQL URL including an explicit database name.
2. Opens an SQLx pool to that database.
3. Applies pending migrations.
4. Validates that the single server instance exists.

The supplied URL must use the PostgreSQL URL scheme. The crate does not
silently read environment variables, connect to a default database, create
a new database or start PostgreSQL. There is no option in `pos-server` to
connect to PostgreSQL yet.

Use a dedicated POS database and a narrowly privileged DB role. The role
needs enough permissions to create and migrate POS's schema, so protect its
credentials. Avoid embedding credentials in shell command arguments or logs.
When using PostgreSQL over an untrusted network, require TLS in its connection
URL; the disposable CI database alone intentionally uses non-TLS loopback.

Migrations change the selected database schema and are not automatically
reversible. Back up a real database before any migration. Do not test
migrations against user data, and never run the migration tests against
a production database.

## Test strategy

`cargo test -p pos-postgres --locked` runs unit tests without a database.

Two integration tests need an explicit `POS_TEST_DATABASE_URL` and refuse to
run against a database not named `pos_ci` or hosted outside localhost.
On a matching disposable database, it concurrently opens two connection
pools, tests migration locking/idempotence, verifies a stable instance UUID,
and confirms that user/device/session tables exist while Tasks, Projects and workspace tables do not. The other tests password sign-in, device ownership, expiry and revocation.

GitHub Actions starts an ephemeral `postgres:17-alpine` service with dedicated
throwaway test credentials and runs this integration test. This does not
connect to or modify any homelab database.

## Next steps

- Review the implemented identity and credential model, then add authenticated provisioning and account lifecycle interfaces behind rate limiting and HTTPS.
- Tenant/workspace-scoped server tables and storage interfaces, after
  ownership rules are explicit.
- Wire the authenticated server service to PostgreSQL.
- Add business-data routes only after authentication and permissions exist.

The HTTP health endpoint remains a process-only probe. PostgreSQL health
must not be represented as functional multiuser readiness.
