# PR #18 - Offline synchronization foundations

Status: development branch. **No synchronization is active.** These changes
prepare the POS SQLite client for safe, offline-first synchronization.
There is no server upload, download, mapping, acknowledgement, automatic merge,
conflict resolution, background job, or new CLI command.

## Database migration and safety

When a **new version of POS opens a local SQLite database for writing**, SQLx
applies `20261114000000_sync_change_journal.sql`. This is an **additive,
persistent migration**. It creates new columns, a journal table, indices and
triggers, and records the current state of all existing Tasks/Projects in
the journal. It does not replace the legacy integer IDs, task/project fields,
relationships, UI, or user records.

**Back up the existing database before first launching this build on real
data.** Migrations are not automatically reversible. If rollback is needed,
retain the original backup and use the supported no-overwrite restore into a
different SQLite path (see [data recovery](data-recovery.md)). Do not manually
delete the journal, columns or SQLx migration history, as doing so could lose
offline changes.

The migration generates a random RFC 4122 v4 UUID in a new `sync_id` column
for each existing Task and Project. New rows receive one through SQLite
triggers, even when inserted directly via SQL. IDs are immutable and unique
within each entity table. Existing integer IDs stay local and unchanged.
**These client-side UUIDs are not the PostgreSQL server record UUIDs.**
The Rust code uses a distinct `pos-entities::LocalRecordId` newtype, not a raw server UUID. A
separate explicitly designed mapping, workspace binding and device identity
will be required before synchronization.

## Offline journal

A new table, `pos_sync_outbox`, stores append-only mutation snapshots:

- `format_version`: fixed at 1 to support future snapshot format changes
- `seq`: increasing cursor **local to this SQLite database** (not a
  globally unique operation identifier)
- `record_kind`: task or project
- `sync_id`: the affected record's stable local UUID
- `operation`: upsert or delete
- `payload_json`: snapshot of record fields for upsert, NULL for a delete
- `created_at`: local journal insertion timestamp (not a version clock)

All local insert/update/delete operations produce journal entries
**in the same SQLite transaction** as the change. Failed or rolled-back
writes leave no journal entry. A deleted record produces a durable tombstone.
Deleting a project also records the resulting task unassignments via the
existing ON DELETE SET NULL relationship. Each upsert snapshot contains a
`project_sync_id` instead of a local integer project ID, plus the
user-visible fields and created/updated timestamps. The journal preserves
multiple edits rather than silently compressing them.

Existing rows are seeded with one upsert snapshot, with Projects preceding
Tasks so their stable IDs already exist. Seeding happens only once when the
migration is applied. Later launches do not regenerate IDs or snapshots.
Changes are never acknowledged, removed or compacted in this release;
SQLite triggers also reject accidental journal UPDATE/DELETE statements.

This preserves **plaintext copies of changed task/project contents** in the
same SQLite database. Journal growth is unbounded until a future
acknowledgement/retention protocol is safely implemented. Treat the database
and backups as sensitive and monitor disk use. JSON export v2 deliberately
remains unchanged and **does not include the outbox**; use SQLite backup
and no-overwrite restore to preserve pending journal state.

## Read-only code interfaces

- `SqliteTaskRepository::task_sync_id(id)` and
  `project_sync_id(id)`: lookup without modifying data
- `pending_sync_changes(after_seq, limit)`: page through ordered,
  validated snapshots, exclusive cursor, limit 1..=1000
- `pos-sync::PendingChange`, `RecordKind`, `ChangeOperation`
- `pos-sync::compare_revisions(expected, observed)` returns
  `SafeToAttempt` only when a record is absent at both ends or when
  positive version numbers match; any divergent or unknown base is a
  **conflict**, never permission to overwrite the server

The revision comparison is an **offline, pure function**, not an upload
engine or a substitute for transactional server version checks. The
client does not yet store server baseline versions or conflict snapshots,
so no write is actually attempted. No remote PostgreSQL data is accessed.

## Known unfinished requirements for sync

Before enabling upload/download, we need explicit workspace membership
binding, per-device identity, UUID mapping and server record import,
idempotency keys for retries, server-side version checks, a durable
acknowledgement/high-watermark protocol, quarantine and resolution for
conflicting versions, delete protection, and an ordering/replay strategy.
Copied/restored SQLite databases contain the same local IDs and events;
**never synchronize two writable copies of a restored database as separate
devices** until clone/device fencing is implemented.

## Verification

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test -p pos-sqlite --test sync_journal --locked
cargo build --workspace --locked
```

Disposable integration tests verify prior database migration and once-only
seeding; stable IDs across edits/reopen; immutable historical snapshots;
direct SQL writes; failed statements and rollback atomicity; delete
tombstones; project-to-task unlink journaling; pagination and validation;
append-only enforcement; backup/restore preserving IDs and journal.
There is also a pure conflict-policy unit test. The tests never touch
live POS data or require network access.

## Rollback

Before first running a new build against real data, make a normal SQLite
backup with the existing, pre-PR18 client. If a rollback is required,
run the documented no-overwrite restore to a **new path**, select it via
`POS_DB_PATH` and use the older POS build with that path. Any edits made
after the backup cannot be recovered by that older backup alone. Do not
downgrade an already-migrated database in place.
