# pos-postgres: crate-specific instructions

Read and follow the workspace root AGENTS.md. This is a library crate.

## Responsibility

PostgreSQL implementation for authoritative server-side POS state.

## Boundaries

Keep database-specific SQL here; do not weaken tenant or workspace isolation.

## Verification

Future tests should cover transactions, concurrent writes and authorisation boundaries.
The current crate is a compiling scaffold only; do not imply that its future features exist.
