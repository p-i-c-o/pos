# PR #17 - Trusted account provisioning and device onboarding

This is a **development-only** account and device sign-in flow for POS. It
reuses the existing Argon2id password hashing, random 256-bit session tokens,
hashed PostgreSQL session storage, UUID device identity, permission checks and
24-hour expiry. **No public account registration, TLS deployment, web UI or
synchronization has been implemented.**

## Operator: create a POS account

On the machine administering a **dedicated** POS PostgreSQL database, with
authorized database credentials supplied through the operator's protected
environment:

```sh
pos-server admin create-user --email user@example.test
```

The program prompts for a new password (minimum 12 bytes), without echo, and
requests confirmation. It creates the user and a **private Personal workspace
atomically**, so failed workspace creation also rolls back the new account.
Only a trusted POS operator with access to `POS_SERVER_DATABASE_URL` can
invoke this CLI. There is **no HTTP account-creation endpoint**.

An operator can revoke access explicitly:

```sh
pos-server admin revoke-device --user USER_UUID --device DEVICE_UUID
pos-server admin disable-user --user USER_UUID --yes
```

Disabling a user is high impact: it immediately invalidates all sessions and
prevents future sign-in. Device revocation invalidates all sessions on that
device. These commands do not delete any tasks/projects or database rows.

**Caution:** The administrator CLI opens the configured POS database and
applies existing migrations. On any existing POS database, back up before
migrating. It never discovers, creates or changes a database elsewhere.

For a controlled automated bootstrap, `--password-stdin` is available instead
of a prompt. Passwords must never appear in command-line arguments, log
messages or Git files.

## Client: sign in once per device

Run a POS server with `POS_SERVER_DATABASE_URL` configured and bound to
loopback. From the client machine, use the same machine's loopback endpoint
or establish an **SSH local port forward** to the server. For example,
with a loopback POS server on a trusted SSH host:

```sh
ssh -N -L 8787:127.0.0.1:8787 your-pos-server-ssh-host
```

In another terminal:

```sh
pos remote --url http://127.0.0.1:8787 login \
    --email user@example.test --device-label ThinkPad
```

POS prompts for the password without echo. On success it enrolls a device,
saves a 24-hour bearer session in
`$XDG_CONFIG_HOME/pos/session.token` (fallback:
`~/.config/pos/session.token`) and stores the device UUID separately in
`session.device`. Both files are owner-only 0600 in a 0700 directory.
These are **session secrets**, not POS Tasks/Projects database records.

On subsequent sessions, use ordinary remote commands without setting
`POS_SERVER_TOKEN` or copying session tokens:

```sh
pos remote me
pos remote workspaces
pos remote projects WORKSPACE_UUID
pos remote tasks WORKSPACE_UUID --json
```

After 24 hours, repeat login with an explicit replacement:

```sh
pos remote login --email user@example.test --replace
```

POS reuses the saved device UUID, issues a new token, atomically replaces
the session file and attempts to revoke the previous token. If that final
revocation fails, POS warns that the prior session may remain valid until
expiry. The old file is not truncated on a failed login.

To intentionally enroll a different device identity after a revoked device,
use `pos remote login --email user@example.test --replace --new-device`.

To revoke the current session and remove the local session file:

```sh
pos remote logout
```

The device UUID file remains, enabling future login on that same device.
If a session has already expired or was revoked remotely, logout returns
401 without deleting the local file. Use `login --replace` to authenticate
again; there is no silent local deletion.

An explicit `POS_SERVER_TOKEN_FILE` overrides the default file location
for login, reads and logout. The `POS_SERVER_TOKEN` environment variable is
still supported for legacy development-only read commands, but should be
unset before interactive login.

## Protocol and security constraints

The server adds two routes only when a POS PostgreSQL database is configured:

- `POST /api/v1/auth/login` with `email`, `password`, `device_label` and
  optional `existing_device`. Returns `user_id`, `device_id`, a one-time
  token, and `expires_in_seconds: 86400`, with `Cache-Control: no-store`.
  Request body is limited to 2 KiB. Failed password or revoked device yields
  401. The login route has a conservative **global 8 attempts per 60 seconds**
  limit (429) in memory, not a production anti-abuse service.
- `POST /api/v1/auth/logout`, authenticated by the current bearer token,
  revokes only that session and returns HTTP 204 with `no-store`.
  A missing/expired/revoked token gets 401.

The server remains **loopback-only**. The client accepts only literal
`http://127.0.0.1:PORT` or `http://[::1]:PORT`; no arbitrary LAN/public
hosts, proxies, HTTP redirects or unencrypted remote access are allowed.
When connecting across machines, SSH must protect the forwarding path.
The login password should only cross the local loopback connection and
the authenticated SSH tunnel.

The client never prints tokens or passwords, never accepts passwords on the
command line, uses bounded responses/timeouts, and does not create SQLite
files. It does not sync or merge local and remote data.

**This is NOT a production authentication service.** Future work requires
hardened network transport/TLS, per-account/IP rate limits, persistent login
abuse controls, deployment secrets, session lifecycle improvements, audit
logs, verification/recovery workflows and device management inside the TUI.

## Verification

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --locked
cargo test -p pos-cli --test onboarding --locked
```

CI additionally runs `cargo test -p pos-server --test onboarding` with an
isolated PostgreSQL 17 `pos_ci` instance. The test refuses non-`pos_ci`
and non-loopback hosts, validates account/workspace creation, correct and
incorrect login, device reuse, session revocation, workspace discovery,
wrong-device rejection and HTTP 429. Client tests use disposable mock
loopback servers, private temporary token files, and explicitly verify that
no local SQLite database is initialized.

No real POS account, database, homelab service, token or network configuration
is modified merely by building or testing this PR.
