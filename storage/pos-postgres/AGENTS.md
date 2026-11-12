# pos-postgres: crate-specific instructions

Read and follow the workspace root AGENTS.md. This is a library crate.

## Responsibility

PostgreSQL implementation for authoritative server-side POS state.

## Boundaries

Keep database-specific SQL here; do not weaken tenant or workspace isolation.
Persistent POS-owned records include server instance metadata, security
identities (users, devices and hashed sessions), and workspace ownership and
explicit membership tables. Do not add Task or Project server data until
authorization is integrated with those operations.
Never return raw password hashes or tokens to HTTP clients.

## Verification

The connection, migrations and identity operations have unit and PostgreSQL
integration tests against a disposable localhost database, including password
login, device ownership, expiry and revocation. Future data repositories must
test transactions, concurrent writes and tenant isolation.
The crate is used by optional read-only identity/workspace HTTP endpoints but NOT by any business-data operations.
