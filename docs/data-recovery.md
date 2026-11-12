# POS local data recovery

This is the supported manual recovery workflow for POS's **local** SQLite tasks
and projects. It does not cover server storage, cross-device sync, retention
automation or encrypted backups, none of which are implemented yet.

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

The JSON format has format_version: 1 and includes all local projects and tasks,
including archived/completed records and task-project IDs. Both collections
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

Verification is covered by SQLite backend and CLI integration tests with
separate file paths, live WAL-mode data, existing destination sentinels,
corruption detection and independent recovered database writes.
