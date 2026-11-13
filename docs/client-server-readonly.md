# PR #16 - Read-only POS client-server connectivity

This is a **development-only** client transport. The `pos` binary now
supports authenticated, read-only inspection of a running POS server over
HTTP **on a loopback IP only**. Local Tasks/Projects, SQLite, TUI and
maintenance commands are unchanged.

## Requirements

- POS server listening on a local loopback TCP port, with
  `POS_SERVER_DATABASE_URL` configured to a dedicated POS PostgreSQL
  database; a server started without PostgreSQL returns 404 for the
  authenticated API.
- A valid, device-bound bearer session token **already issued by trusted
  POS provisioning code**. The POS server does not currently have public
  login, registration, token issuance, or device enrollment endpoints.
- For a server on another machine, forward a local TCP port over SSH,
  then use `http://127.0.0.1:PORT` as the client server URL.
  POS does not yet directly connect over TLS or to arbitrary hosts.

## Read-only CLI

```sh
# Configure connection for this shell (defaults to http://127.0.0.1:8787).
export POS_SERVER_URL=http://127.0.0.1:8787

# Recommended: supply an existing, valid session token through a private
# regular file, readable only by the owner (0600). Do NOT put the token
# in a CLI argument, repository, shell history or shared config.
export POS_SERVER_TOKEN_FILE="$HOME/.config/pos/session.token"

pos remote me
pos remote me --json
pos remote workspaces
pos remote workspaces --json
pos remote projects <workspace-uuid>
pos remote projects <workspace-uuid> --json
pos remote tasks <workspace-uuid>
pos remote tasks <workspace-uuid> --json

# Command-scoped URL alternative:
pos remote --url http://127.0.0.1:8788 workspaces
```

If a token file is unavailable, `POS_SERVER_TOKEN` can supply the token
through the environment for testing. `POS_SERVER_TOKEN_FILE` takes
precedence. Avoid exporting long-lived tokens into broad environments.
The token file must be a regular file with no group/other permission bits.
The client rejects missing, malformed, or oversized credentials and never
prints tokens or raw remote error bodies.

All `remote` commands run **before any SQLite database path is created,
opened or migrated**. Remote data is printed only; it is not persisted,
merged, cached, queued for sync, or converted into local task/project IDs.
They do not create a local configuration file.

## Security and failure behavior

The client deliberately accepts only explicit HTTP loopback addresses:
`127.0.0.1` and `[::1]`, with a port and no path, query, userinfo or
fragment. No proxies or HTTP redirects are followed. A bounded timeout
prevents permanent hanging and successful JSON response bodies are capped
at 1 MiB. Untrusted record text is sanitized for terminal display; JSON
mode preserves the returned data.

- 401: invalid, expired or revoked session
- 404: inaccessible workspace or API not enabled
- 503: remote database unavailable
- Connection refused/timeout: server not reachable
- Other responses or incompatible JSON: command fails without printing
  response bodies, URLs containing credentials, or auth tokens

Project and Task lists currently include only the server's **first 200
records**. These commands are NOT a remote backup/export mechanism.
There is no remote write path, no pagination/filtering, no automatic retry,
no sync and no conflict resolution.

## Verification

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --locked
cargo test -p pos-cli --test remote --locked
```

The client integration tests run an ephemeral loopback mock HTTP server,
verify the bearer header and read-only routes for identity, workspaces,
projects and tasks, error handling and that the `remote` command never
initializes a local SQLite database. They require no real token,
PostgreSQL instance or reachable homelab endpoint.

**Next:** provision users/devices through a trusted, limited onboarding
process, followed by designed sync identities/change queues and secure
remote connectivity. Do not expose the present loopback-only POS server
through an unauthenticated public reverse proxy.
