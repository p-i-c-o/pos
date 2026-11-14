# pos-sqlite: crate-specific instructions

Read and follow the workspace root AGENTS.md. This is a library crate.

## Responsibility

SQLite implementation for client-local and offline POS state.

## Boundaries

Keep database-specific code here; changes to schemas and migrations need approval.

## Verification

Future tests should cover transactions, offline recovery and migration safety.
SQLx has additive Tasks, Projects and Tasks v2 migrations plus consistent, validated, no-overwrite backup/restore using VACUUM INTO. Do not edit applied migrations; projects must not cascade-delete tasks. Read docs/data-recovery.md before changing maintenance semantics.

## Local sync foundation (PR #18)

A new additive migration provides immutable UUID v4 `sync_id` values
for Tasks and Projects plus transactional `pos_sync_outbox` triggers.
All existing rows are seeded once. Trigger-based snapshots include
direct SQL writers and FK unassignment, and a tombstone survives delete.
The outbox is append-only; no acknowledgement, network, cleanup or sync
worker is implemented. Local int IDs and JSON export v2 stay unchanged.
Read docs/sync-foundations.md and docs/data-recovery.md before editing
schema or these triggers. Test rollback, foreign-key changes,
pre-migration data preservation and backup/restore against disposable DBs.

## One-workspace identity binding (PR #19)

The new additive sync-identity migration adds an immutable singleton
database UUID, one immutable workspace binding, and a durable local-to-server
record mapping with positive monotonic revisions. Binding requires a separate
authenticated server read and a path-specific private anchor created by the
CLI. No server writes or sync are implemented. Restore/copy must not silently
switch device, workspace or database identity. Tests: sync_identity plus
sync_journal. Read docs/sync-identity-binding.md before changing these tables.
