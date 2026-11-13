# Authenticated HTTP and workspace scopes (Phase 9c)

Status: historical Phase 9c design notes. The tagged v0.1.0 release is not
modified. The Rust TUI still uses local SQLite. PR #17 subsequently added
[development-only account onboarding](account-onboarding.md) over loopback
HTTP; statements below about absent login routes describe Phase 9c.

## Behavior

The POS server can operate in two modes:

- **Default:** No database setting. It listens only on localhost and
  provides the existing process-health endpoints. All authenticated API
  paths return 404. No database is opened or modified.
- **Opt-in PostgreSQL:** Set `POS_SERVER_DATABASE_URL` to the URL of a
  dedicated POS PostgreSQL database before starting the server. The server
  validates the loopback listener, connects to PostgreSQL, applies pending
  SQLx migrations, and enables **read-only**, session-authenticated routes.
  An invalid URL, missing database, or failed migration prevents startup.

Do not point `POS_SERVER_DATABASE_URL` at an existing non-POS database.
**Migrations change the selected database schema and are not automatically
reversible. Back up any database before enabling this mode.** No PostgreSQL
service is installed or configured automatically. Keep credentials in a
protected environment/secret mechanism, never in a committed file or logs.

## HTTP routes

Only available with an explicitly configured PostgreSQL database:

| Method | Path | Meaning |
| --- | --- | --- |
| GET | `/api/v1/auth/me` | UUIDs of the authenticated user and device |
| GET | `/api/v1/workspaces` | Workspaces this user may read |
| GET | `/api/v1/workspaces/{id}` | One accessible workspace, otherwise 404 |

Clients must send exactly one `Authorization: Bearer <token>` header with a
valid, unexpired, non-revoked, device-bound token issued by trusted POS code.
Unknown or revoked tokens get 401. PostgreSQL errors get 503; an inaccessible
workspace is indistinguishable from an unknown workspace (404). Successful
responses and authorization errors have `Cache-Control: no-store`.

Each workspace response contains its UUID, name, `personal` or `shared` kind,
`owner` / `editor` / `viewer` role, and effective read/write/member-management
flags. This is access **metadata**, not access to actual Tasks or Projects.

The existing `/health` and `/api/v1/health` endpoints remain unauthenticated
process-liveness probes, not readiness/identity checks. The server remains
loopback-only even when the database is configured.

## Authorization

- An authenticated identity alone grants **zero** workspace permissions.
- Personal workspaces: only their owner may access them.
- Shared workspaces: owner has all workspace permissions. Explicit viewer
  memberships allow reading metadata; editor memberships allow reading and
  writing *under future authorized business operations*. Neither may manage
  members or delete workspaces.
- Membership grants and removals are trusted-only database service calls;
  they require the workspace owner and cannot grant `owner`.
- Access is checked by scoped PostgreSQL queries for the authenticated
  user, not by fetching all records and filtering afterward.
- Suspended users, revoked devices, expired sessions and revoked tokens
  cannot authenticate. Permission changes become effective on subsequent
  requests, without restarting the server.
- No cross-workspace or inherited organisational privileges exist yet.

There are **no HTTP registration, password login, token issuance,
workspace creation, membership-management or Tasks/Projects endpoints**.
The prior trusted-only provisioning/sign-in methods remain internal and
are used in isolated CI tests. Do not expose through an internet-facing
proxy, SSH tunnel or mesh VPN. HTTPS, rate limiting, account-management
workflows and full tenant authorization will be required before this can
serve remote clients.

## Testing

Local commands:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --locked
python3 scripts/server-smoke.py ./target/debug/pos-server
```

GitHub Actions additionally provisions a disposable PostgreSQL 17 database
named `pos_ci` and runs `cargo test -p pos-server --test authenticated`.
It verifies active bearer sessions, missing/malformed/duplicate authorization
headers, per-user workspace visibility, 404 for other owners' workspaces,
viewer/editor permissions, owner-only membership management, immediate
membership removal, revoked sessions, disabled users, and absence of
HTTP writes. Test URLs are checked for the local `pos_ci` database before
any migrations or data writes.

## What remains

Authenticated server Tasks/Projects create/read/update is now implemented in
Phase 9d ([details](server-records.md)); cross-device synchronization remains
unimplemented. The permission model here is deliberately minimal. Before
adding business data, implement ownership-aware repository interfaces,
transaction-level authorization, stronger tenancy guarantees, and security
tests. Later plan account provisioning/login flows, HTTPS/rate limiting,
audit logs, and trusted deployment configuration.
