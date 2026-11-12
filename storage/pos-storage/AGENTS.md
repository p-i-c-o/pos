# pos-storage: crate-specific instructions

Read and follow the workspace root AGENTS.md. This is a library crate.

## Responsibility

Backend-neutral storage interfaces shared by POS clients and server.

## Boundaries

Do not place SQLite or PostgreSQL SQL in this crate, and do not invent persistent schemas.

## Verification

Future tests should define backend conformance expectations.
The current crate is a compiling scaffold only; do not imply that its future features exist.
