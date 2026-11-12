# pos-postgres: crate-specific instructions

Read and follow the workspace root AGENTS.md. This is a library crate.

## Responsibility

PostgreSQL implementation for authoritative server-side POS state.

## Boundaries

Keep database-specific SQL here; do not weaken tenant or workspace isolation.
Persistent POS-owned records include server-instance metadata and security
identity tables (users, devices, hashed sessions). Do not add Task, Project,
workspace or tenant data before scoped ownership and permissions are defined.
Never return raw password hashes or tokens to HTTP clients.

## Verification

The connection, migrations and identity operations have unit and PostgreSQL
integration tests against a disposable localhost database, including password
login, device ownership, expiry and revocation. Future data repositories must
test transactions, concurrent writes and tenant isolation.
The crate is not yet connected to the HTTP server or business-data operations.
