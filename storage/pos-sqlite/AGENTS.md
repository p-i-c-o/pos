# pos-sqlite: crate-specific instructions

Read and follow the workspace root AGENTS.md. This is a library crate.

## Responsibility

SQLite implementation for client-local and offline POS state.

## Boundaries

Keep database-specific code here; changes to schemas and migrations need approval.

## Verification

Future tests should cover transactions, offline recovery and migration safety.
The current crate is a compiling scaffold only; do not imply that its future features exist.
