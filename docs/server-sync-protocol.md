# PR #20 - Server synchronization protocol

Development-only, loopback-only authenticated Tasks/Projects protocol. No client automatic synchronization, remote deployment, delete, tombstones or conflict merging is enabled.

## Read incremental changes

`GET /api/v1/workspaces/{workspace}/sync/changes?after=0&limit=100`

Requires a valid device-bound bearer session and workspace read permission. Response has `changes` (cursor, record_kind, record_id, revision and complete JSON snapshot), `next_cursor`, `has_more`. Use `next_cursor` as the next `after`. Default and maximum page size: 100, minimum 1. Cursor 0 means the beginning. The cursor is scoped to one workspace and must never be reused for another workspace.

The additive PostgreSQL migration backfills existing server Tasks and Projects as initial snapshots. Subsequent inserts/updates are appended in the same transaction by triggers. A per-workspace counter lock serializes change writes, so cursor order tracks commit order. There are no deletes and no journal pruning in this version.

## Submit an idempotent operation

`POST /api/v1/workspaces/{workspace}/sync/operations`

Requires a valid device-bound bearer session and workspace write permission. Body (example):

```json
{"operation_id":"11111111-1111-4111-8111-111111111111","change":{"type":"create_project","name":"University"}}
```

Supported change types: `create_project` (`name`, optional `slug`), `update_project` (`record_id`, `expected_version`, `name` and/or `archived`), `create_task` (`title`, optional `description`, `due_date`, `project_id`, `priority`, `status`), and `update_task` (`record_id`, `expected_version`, and task-edit fields). Nullable task fields can be explicitly cleared with `null`.

Successful response includes `operation_id`, `record_kind`, `record_id` (server UUID), `revision` and `cursor`. The server atomically saves the mutation and the receipt in PostgreSQL. Retrying the same operation ID with the same payload from the same authenticated device and workspace returns the identical receipt. A different payload with an existing operation ID returns HTTP 409. Concurrent identical retries are serialized by the operation receipt's unique key. Operation receipts do not expire automatically.

Client identity is taken from the validated session, never from the request. All writes recheck write scope in PostgreSQL. Stale versions and unique constraint conflicts return HTTP 409, requiring a fresh read before a new edit. Inaccessible workspaces/records return HTTP 404. An invalid request returns HTTP 400. The endpoint does not merge divergent edits, acknowledge local SQLite journal entries, or perform client enrollment.

## Safety, limitations and migrations

Migrations `20261114020000_sync_change_feed.sql` and `20261114021000_sync_operation_receipts.sql` add PostgreSQL tables, triggers and metadata. They backfill existing server records but do not change local SQLite data. They must be applied only to a backed-up dedicated POS server database. They are not automatically reversible.

The server transport remains loopback-only. No TLS or remote production exposure. The feed contains full record snapshots, and its durable history grows with every edit. There are no hard-delete/tombstone operations, automatic history retention, multi-device clone detection, or full client-side download/upload engine. Future sync work must address safe restore and identity epochs before enabling remote clients.

## Verification

Run `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace --locked`, and `cargo build --workspace --locked`. PostgreSQL integration tests run only when `POS_TEST_DATABASE_URL` names the disposable local `pos_ci` database, as configured in CI.
