# pos-sqlite: crate-specific instructions

Read and follow the workspace root AGENTS.md. This is a library crate.

## Responsibility

SQLite implementation for client-local and offline POS state.

## Boundaries

Keep database-specific code here; changes to schemas and migrations need approval.

## Verification

Future tests should cover transactions, offline recovery and migration safety.
The first Tasks SQLx migration and adapter are implemented. Never change an applied migration in place; add a new migration after approval.
