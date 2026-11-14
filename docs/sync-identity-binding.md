# PR #19 - Local database identity and explicit workspace binding

Status: development only. PR #19 adds **identity and binding metadata** for
future synchronization. It does not enable uploads, downloads, acknowledgements,
conflict resolution, server writes, a sync job or TUI integration.

## User workflow

1. Back up any existing POS database before first launching this build. A new
   additive SQLite migration is applied during ordinary Tasks/Projects startup
   or during the explicit `sync bind` command. It is not reversible in place.
2. Sign in on a client with the development-only POS server reachable through
   loopback or an SSH local port forward.
3. List accessible workspaces and choose exactly one workspace UUID.
4. Bind the local SQLite database to it **explicitly**.

```sh
# The following examples use the usual local database path.
pos remote login --email user@example.test --device-label ThinkPad
pos remote workspaces

# Workspace is the UUID obtained from the server's authenticated list.
pos sync bind WORKSPACE_UUID --url http://127.0.0.1:8787

# Local-only introspection, without contacting the server or migrating a DB:
pos sync status
pos sync status --json
```

All commands support the existing global `--db PATH` or `POS_DB_PATH`
override. The server URL is limited to literal HTTP loopback IPs and can
point through a trusted SSH local port forward. `POS_SERVER_URL` is accepted
when `--url` is omitted.

`pos sync bind` performs authenticated **GET** requests to
`/api/v1/auth/me` and `/api/v1/workspaces`. It refuses a workspace the
current account cannot read. Only after successful server verification does
it open/migrate the local SQLite DB and persist a single binding. It records
the verified server user UUID, device UUID, workspace UUID and exact server
origin. The server is never changed.

The same bind can be repeated without changing its state. Switching the
account, device, workspace or server address **is not supported** and is
refused. Choosing the server origin includes the loopback port: use a stable
forwarded port for now. Do not delete or overwrite binding metadata to
circumvent the safety check.

## Database schema

`20261114010000_sync_identity_binding.sql` is strictly additive:

- `pos_sync_database`: a singleton, immutable random UUID v4 per SQLite
  database, generated on migration. It is distinct from both local record
  sync IDs and server record UUIDs.
- `pos_sync_binding`: an immutable, at-most-one workspace binding including
  database ID, verified user/device/workspace UUIDs, server origin and the
  database's resolved canonical filesystem path.
- `pos_sync_record_map`: internal local-record to server-record mapping
  table, with uniqueness enforced separately for each record type and a
  positive server revision. Neither the migration nor `sync bind` creates
  any mapping entries. Current local Tasks/Projects and the append-only
  journal remain intact.

Internal storage APIs can add a mapping after a future **verified server
response** and advance its server revision only with a monotonic
compare-and-swap. Mappings cannot be reassigned or deleted. These APIs are
not exposed through the CLI or connected to the network. No base revision
is implicitly guessed from an unmapped local record.

## Copy and restore safeguards

In addition to the immutable SQLite database identity and binding, the
client writes a private JSON **sync anchor** outside the SQLite database:

```text
$XDG_CONFIG_HOME/pos/sync-anchors/<database-uuid>.json
# fallback: ~/.config/pos/sync-anchors/<database-uuid>.json
```

The anchor is owner-only `0600` in `0700` directories. It records the
database UUID, resolved database path, server origin and verified user,
device and workspace identities. A bound database can be inspected only
when the external anchor exactly matches. Missing, copied, edited, unsafe
or mismatched anchors cause the `sync status` and `sync bind` commands
to **fail closed**. The application does not automatically replace a missing
anchor or silently create a new database identity.

A normal SQLite backup/restore preserves the database ID, binding, maps and
journal but **does not include the external anchor**. A restored copy at a
new path therefore cannot be bound as a second writable device. Likewise,
moving a bound DB to a new path or another computer requires an explicitly
designed recovery/re-enrollment process, not a blind `sync bind`.

This is a **best-effort local clone fence**, not cryptographic clone
prevention. Someone who deliberately copies *both* the database and external
anchor to an identical path could still impersonate the local installation.
Future server-backed sync must add durable device epochs, replay protection
and server-side clone detection **before any sync writes are enabled**.
The anchor and SQLite DB should be protected and backed up deliberately;
do not confuse this with a complete restore solution.

## Recovery and limitations

- Existing Tasks/Projects, their original integer IDs, local record UUIDs,
  JSON export format and append-only journal are not rewritten.
- Status is read-only and does not apply migrations. Run a normal local
  command after backup to migrate a legacy database before using status,
  or invoke the explicit binding operation with valid server credentials.
- Binding a database does not upload any journal entries, assign server IDs,
  or modify the server. The durable journal continues accumulating locally.
- No unbind, rebind, database clone migration, workspace reassignment or
  server session refresh is implemented.
- If the machine or the bound database is restored, avoid deleting anchors
  or editing SQLite tables. Preserve the original and use the documented
  no-overwrite backup/restore workflow. A supported migration/recovery
  workflow must be added before enabling synchronization.
- The current client/server transport is loopback-only. Production TLS and
  authenticated remote deployment are separate work.

See [PR #18 sync foundations](sync-foundations.md) and
[POS local data recovery](data-recovery.md).

## Verification

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test -p pos-sqlite --test sync_identity --locked
cargo test -p pos-cli --test sync_binding --locked
cargo build --workspace --locked
```

Disposable tests cover PR #18-to-#19 migration, once-only DB identity,
backup/restore and unchanged task/project data, immutable binding,
server-UUID mapping uniqueness, version CAS, auth preflight over an HTTP
mock, denied workspace with no SQLite migration, idempotent binding,
0600/0700 anchor permissions, moved database and missing-anchor fencing.
No real PostgreSQL installation, server data, homelab database or credentials
are changed by these tests.
