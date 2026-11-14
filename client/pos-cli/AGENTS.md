# pos-cli: crate-specific instructions

Read and follow the workspace root AGENTS.md. This is a binary crate.

## Responsibility

The single pos client executable, its argument parsing and TUI launch wiring.

## Boundaries

Do not implement module business rules here. Route operations through shared core and validated interfaces.

## Verification

Future tests should cover argument parsing, exit codes and non-interactive output.
The pos executable supports Tasks (including v2 search/sort/metadata), Projects and Data commands and launches Ratatui. Route operations to shared services and SQLite maintenance; do not add database logic to rendering or block the TUI thread.

## Read-only server access

The `remote` CLI command is separate from local SQLite operations and
runs before local database startup. It inspects only authenticated identity,
accessible workspaces, remote projects and remote tasks via the server's
existing GET API. It never writes server records, converts remote UUIDs into
local integer IDs, caches remote responses, or starts synchronization.

No credentials in CLI arguments, configuration files tracked by Git, logs
or error bodies. Read a device-bound bearer token from a permission-restricted
token file, or from environment only in development. Explicit HTTP loopback
address only, no redirects or proxies; no public or LAN exposure. Preserve
bounded response handling, timeouts and terminal text sanitization.
Test via disposable loopback mocks and preserve local data isolation.
See docs/client-server-readonly.md.

## Device onboarding

`pos remote login` uses a hidden password prompt and the loopback server
to enroll or reuse devices. Session tokens live in owner-only 0600 files
in 0700 directories; explicit `--replace` is required for rotation.
`pos remote logout` revokes remotely before deleting the local token.
No secrets in arguments, logs or Git. These commands operate independently
of SQLite. See docs/account-onboarding.md.

## Local sync binding (PR #19)

`pos sync bind` does two authenticated GET requests to verify the selected
server workspace and device identity *before* opening/migrating SQLite.
It then reserves a 0600 anchor under the private XDG config directory
and inserts an immutable one-workspace binding. `pos sync status` is
local read-only, checks the external anchor and must not migrate or create a
database. Never auto-rebind, fabricate mappings, restore missing anchors,
or enable network sync without a separately reviewed protocol.
See docs/sync-identity-binding.md.
