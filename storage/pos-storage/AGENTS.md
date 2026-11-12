# pos-storage: crate-specific instructions

Read and follow the workspace root AGENTS.md. This is a library crate.

## Responsibility

Backend-neutral storage interfaces shared by POS clients and server.

## Boundaries

Do not place SQLite or PostgreSQL SQL in this crate. Define backend-neutral contracts only; schema changes require approval.

## Verification

Future tests should define backend conformance expectations.
TaskRepository defines the async Task persistence contract, currently implemented by pos-sqlite.
