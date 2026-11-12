# User and device authentication foundation (Phase 9b)

**Development-only, internal APIs.** This branch introduces identity storage and
credential-verification primitives. **It does not enable user signup or password login over HTTP, or task/project
access.** The Axum server now optionally accepts existing bearer sessions for
read-only identity/workspace metadata on localhost, using PostgreSQL.

## Identity model

- A **user** is a unique canonical ASCII email and a per-user salted Argon2id
  password hash, plus creation and optional disable timestamps.
- A **device** belongs to exactly one user. A device has its own server-generated
  UUID, a nonempty label, and an optional revocation timestamp.
- A **session** belongs to exactly one user and one of their devices. The
  client receives a random 256-bit bearer token **once**. PostgreSQL stores
  only its SHA-256 digest, expiry, and optional revocation timestamp.
- Sessions expire after **24 hours**. Every token check joins the user and
  device records and refuses expired/revoked sessions, revoked devices, and
  disabled users. A valid session identifies a user and device only; it does
  **not** imply access to any particular workspace or resource.
- All passwords, tokens, and database connection details are excluded from
  debug output and error strings. In-memory token strings are zeroed on drop.

Password hashes use Argon2id with a random salt, 19 MiB memory cost, 2
iterations, and one lane. PHC strings retain the exact algorithm parameters
so a future upgrade can verify older hashes.

## Internal operations

The `pos-auth` crate validates identities, hashes/verifies passwords,
generates random tokens, redacts credentials, and hashes token strings.

Trusted service code can call `PostgresStore::provision_user` and
`PostgresStore::sign_in`. Successful password login enrolls a new device, or
reuses an explicitly supplied active device UUID *belonging to that user*,
and returns a revocable session. `authenticate_token` checks token validity;
`revoke_token`, `revoke_device` and `disable_user` invalidate access.

**These are not public HTTP provisioning or sign-in handlers.** They remain
callable only from trusted application code, pending an audited provisioning path, TLS,
rate-limiting, account verification, authorization policies, and security
review. The password login interface is not suitable for exposure through a
public proxy, Tailscale, or a network listener as-is.

Unknown users and wrong passwords return the same category of failure.
Password hashing runs on a Tokio blocking thread, not the async worker.
Database errors do not include credentials or connection details.

## Persistent schema

Migration `20261112234000_auth_identities.sql` creates:

- `pos_users` (UUID, normalized email, salted password hash, disabled state)
- `pos_devices` (UUID, owner UUID, device label, revoked state)
- `pos_sessions` (SHA-256 token digest, user and device ownership,
  24-hour session lifetime, revoked state)

Unique and foreign-key constraints enforce normalized unique login emails,
device ownership, token uniqueness and user/device consistency at the
database level. Existing server-instance UUIDs remain unchanged. This
migration contains no workspace, Tasks, Projects or synchronization tables.

**Schema migration is not automatically reversible.** Only ever run these
migrations on a dedicated POS PostgreSQL database, and back up existing
databases before upgrading. This task does not deploy, create or modify
any homelab database.

## Security boundaries and limitations

Authentication is **not authorization**. There is no workspace membership
model, RBAC/ABAC evaluator, resource permissions, session refresh, password
recovery, second factor, account-verification flow, public registration, or
end-user authentication CLI/API. Password-based sign-in is intended only as
an internal primitive until rate limiting and an authenticated API contract
are specified. There is no credential storage on clients or cross-device
synchronization.

A revoked device may not be reused to sign in. Re-enrollment requires a
successful password login and creates a new UUID. Disabling a user immediately
invalidates all sessions through the verification query.

## Verification

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

GitHub Actions additionally starts an isolated PostgreSQL 17 container and
runs the identity integration test with a dedicated `pos_ci` database. It
covers normal sign-in, duplicate users, wrong credentials, device
ownership, multiple sessions, expiry, revocation, disabled users, and
non-disclosure of credential values. Local tests without PostgreSQL do not
exercise the live database test.

The next phase is to define and test server provisioning and authorization
boundaries, before exposing Tasks/Projects APIs or remote access.
