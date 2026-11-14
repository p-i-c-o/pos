# pos-postgres: crate-specific instructions

Read and follow the workspace root AGENTS.md. This is a library crate.

## Responsibility

PostgreSQL implementation for authoritative server-side POS state.

## Boundaries

Keep database-specific SQL here; do not weaken tenant or workspace isolation.
Persistent POS-owned records include server instance metadata, security
identities (users, devices and hashed sessions), and workspace ownership and
explicit membership tables, plus workspace-scoped Tasks/Projects records.
Authorization must be checked inside SQL for every read or mutation; no
client-provided identity is trusted. Task/project cross-workspace links are
blocked by database foreign keys. Writes require an expected version.
Never return raw password hashes or tokens to HTTP clients.

## Verification

The connection, migrations and identity operations have unit and PostgreSQL
integration tests against a disposable localhost database, including password
login, device ownership, expiry and revocation. Future data repositories must
test transactions, concurrent writes and tenant isolation.
The crate is used by optional authenticated HTTP identity/workspace endpoints and server Tasks/Projects create/read/update. There is no hard delete or sync.

## Trusted onboarding

`provision_user_with_personal_workspace` atomically creates a new account
and owner-only Personal workspace. The old `provision_user` remains intact.
Server login/logout reuse existing password hashing and device-bound
session/revocation methods. Keep identity SQL here, not in HTTP handlers.

## PR #20 server-side protocol

Server PostgreSQL now stores a transactional incremental change feed and
per-device operation receipts, with authenticated GET/POST loopback endpoints.
Client queue draining, automatic upload/download and delete/tombstone handling
are not implemented. See docs/server-sync-protocol.md.
