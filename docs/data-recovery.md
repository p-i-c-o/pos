# POS local data recovery

This is the supported manual recovery workflow for POS's **local** SQLite tasks
and projects. It does not cover server storage, cross-device sync, retention
automation or encrypted backups, none of which are implemented yet.

## Sync migration and queue preservation (PR #18)

The optional development version of POS adds a **new SQLite migration** for
stable local record UUIDs and a durable append-only `pos_sync_outbox` table.
Back up a live database using the existing POS backup command **before**
first opening it with a PR #18 build. This schema change is not automatically
reversed by downgrading the POS binary.

A regular SQLite backup and no-overwrite restore preserve the UUIDs,
journal history and deletion tombstones. The versioned JSON export
continues exporting only the user-facing Tasks and Projects; it does
**not** preserve journal history or stable sync IDs, and is **not** a
substitute for a SQLite backup. Keep multiple backups and never sync two
independently writable clones from the same backup without device fencing.

The journal also records plaintext historical versions of fields and can
grow indefinitely until server acknowledgement/compaction is implemented.
Do not delete outbox rows to save space. See
[sync foundations](sync-foundations.md) for schema details, limitations
and a rollback procedure.

## Bound database recovery (PR #19)

A newly bound SQLite database also has a **private external sync anchor**
under `$XDG_CONFIG_HOME/pos/sync-anchors/` (or
`~/.config/pos/sync-anchors/`). This anchor is deliberately excluded from
SQLite backup files and the JSON export. A restored copy retains its
database UUID, workspace binding and record mapping, but will be
**rejected by the sync binding/status commands if its path or anchor differs**.

This prevents accidentally treating a restored copy as a new writable
sync participant. There is no safe automatic rebind/move flow yet.
Preserve the original database and private anchor, and do not edit the
binding table or discard the anchor to bypass the safeguard.
See [workspace binding](sync-identity-binding.md).

## Database location

By default, POS stores data at:

- $XDG_DATA_HOME/pos/pos.sqlite3 when XDG_DATA_HOME is set
- ~/.local/share/pos/pos.sqlite3 otherwise

Override using --db /path/to/pos.sqlite3 or POS_DB_PATH=/path/to/pos.sqlite3.
Backups and exports **require an existing POS database**; they do not create one.
The parent directory of a backup/restore/export destination must already exist.

## Create and verify a backup

Run these commands from a checkout containing the compiled POS binary:

~~~sh
./target/debug/pos data check
./target/debug/pos data backup "$HOME/pos-backup-2026-11-12.sqlite3"
./target/debug/pos --db "$HOME/pos-backup-2026-11-12.sqlite3" data check
~~~

Use a unique filename each time. The backup operation never overwrites an
existing destination, even an empty file or symlink.

The backup uses SQLite VACUUM INTO for a **transactionally consistent snapshot**
including committed writes in the WAL. It does not copy the active database
file naively, and it does not need the source application to stop. The snapshot
is staged in the destination directory, validated with full SQLite integrity
and foreign-key checks, synced, and only then published without replacement.

The backup includes POS tables and migration history. Unlike JSON export, it
can be restored as a complete local database. Keep multiple independently
stored backups, and periodically test recovering them.

## Recover into a separate database

~~~sh
./target/debug/pos data restore "$HOME/pos-backup-2026-11-12.sqlite3" --output "$HOME/pos-recovered.sqlite3"
./target/debug/pos --db "$HOME/pos-recovered.sqlite3" data check
./target/debug/pos --db "$HOME/pos-recovered.sqlite3" tasks list --all
./target/debug/pos --db "$HOME/pos-recovered.sqlite3" projects list --all
~~~

Restore **never overwrites or mutates the existing POS database**. It validates
both the input and the copied file and publishes only a new path. If an output
file already exists, restore fails. If the input is missing, corrupt, not a POS
database or contains foreign-key violations, restore fails without publishing.

To work with the recovered database without replacing the active one:

~~~sh
POS_DB_PATH="$HOME/pos-recovered.sqlite3" ./target/debug/pos
~~~

Keep the original database and backup until recovery is fully verified.
Do not copy over a live SQLite database, especially while its -wal/-shm
sidecars or POS processes may still be active. To switch permanently, use the
POS_DB_PATH override or plan an offline migration and retain rollback copies.

## Portable JSON export

~~~sh
./target/debug/pos data export > "$HOME/pos-data.json"
./target/debug/pos data export --output "$HOME/pos-data-2026-11-12.json"
~~~

The JSON format has format_version: 2 and includes all local projects and tasks,
including task descriptions, priorities and workflow statuses,
archived/completed records and task-project IDs. This is an explicit format
bump from version 1; software consuming JSON must handle the new fields.
Both collections
are read in **one SQLite read transaction**, for a consistent point-in-time
view. The export is read-only and does not run migrations. For --output, POS
stages and syncs the JSON before publishing without overwriting an existing
destination. Plain shell redirection (>) can overwrite a file: use --output
for overwrite protection.

**JSON import is not implemented**, and JSON is not a substitute for the SQLite
backup. IDs and timestamps are preserved in the export as reference data only.

## Limitations

- These are manual commands, not scheduled or remote backups.
- An integrity check detects SQLite structural damage and broken foreign keys.
  It cannot detect every logically incorrect or accidentally deleted record.
- Backups and exports contain personal information and are not encrypted.
  Protect access and store off-device copies separately.
- SQLx may apply migrations during normal interactive/Tasks/Projects startup.
  Use data backup to create a recovery point before upgrading or migrating.
- Restore supports past databases with POS migration history. Opening a restored
  older database for ordinary POS operations may then apply newer migrations.
  Task Engine v2 adds descriptions, priorities and statuses. Back up an older
  database before first opening it with the new version. The read-only JSON
  export expects the current schema, so migrate a restored copy of an older
  backup before exporting it.

Verification is covered by SQLite backend and CLI integration tests with
separate file paths, live WAL-mode data, existing destination sentinels,
corruption detection and independent recovered database writes.
