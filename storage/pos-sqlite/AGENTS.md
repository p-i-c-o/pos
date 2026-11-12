# pos-sqlite: crate-specific instructions

Read and follow the workspace root AGENTS.md. This is a library crate.

## Responsibility

SQLite implementation for client-local and offline POS state.

## Boundaries

Keep database-specific code here; changes to schemas and migrations need approval.

## Verification

Future tests should cover transactions, offline recovery and migration safety.
SQLx has additive Tasks and Projects migrations plus consistent, validated, no-overwrite backup/restore using VACUUM INTO. Do not edit applied migrations; projects must not cascade-delete tasks. Read docs/data-recovery.md before changing maintenance semantics.
