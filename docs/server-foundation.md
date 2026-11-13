# POS HTTP server foundation

Status: **development-only foundation** (merged into `develop`).
This is not part of the tagged POS v0.1.0 release. The server currently
has **no Tasks/Projects APIs, no database, no user identity and no TLS**.

## Start locally

From the repository root with the pinned Rust toolchain installed:

```sh
cargo run -p pos-server
# In a second terminal:
curl -i http://127.0.0.1:8787/health
```

Default binding: **127.0.0.1:8787**, only accessible from the same machine.
Customize it with CLI options or the environment:

```sh
cargo run -p pos-server -- --listen 127.0.0.1:8788
POS_SERVER_LISTEN=127.0.0.1:8788 cargo run -p pos-server
RUST_LOG=pos_server=debug cargo run -p pos-server
```

CLI `--listen` takes precedence over `POS_SERVER_LISTEN`. IPv6 loopback
`[::1]:8787` is also allowed. Wildcard addresses such as `0.0.0.0:8787`,
IPv4-mapped IPv6 and LAN or public IPs are **rejected before binding**.
This deliberately prevents exposing the unauthenticated foundation.

`--listen 127.0.0.1:0` selects an available ephemeral loopback port. The
actual address appears in startup logs, which are written to stderr without
ANSI formatting. Failed binding, invalid config and port collisions result
in a nonzero exit code.

## HTTP contract

Only read-only process health endpoints exist:

| Route | Method | Result |
| --- | --- | --- |
| `/health` | GET | HTTP 200, JSON status |
| `/api/v1/health` | GET | HTTP 200, same JSON |
| Any other route | Any | HTTP 404, structured JSON error |
| Health route with POST/PUT | POST/PUT | HTTP 405 |

Example JSON (version tracks the built crate):

```json
{"status":"ok","service":"POS","version":"0.1.0","api_version":"v1"}
```

Response includes `Content-Type: application/json` and
`Cache-Control: no-store`. `api_version` indicates the reserved URL
namespace; it is **not a promise of stable Tasks/Projects wire contracts**.

Errors for unknown routes use:

```json
{"error":{"code":"not_found","message":"route not found"}}
```

The health result only confirms an HTTP listener is reachable. It does not
assert database health, authentication availability, sync readiness or an
operational self-hosted service.

## Lifecycle and security

The executable starts an asynchronous Tokio runtime, binds to loopback,
serves Axum routes, logs startup/errors with `tracing`, and gracefully
terminates on Ctrl-C (SIGINT) or SIGTERM. Existing in-flight HTTP requests
can finish as part of the graceful shutdown process.

The server does **not** read or modify a POS SQLite database. It does not
start any workers, automatic jobs or persistence. There are no systemd units,
deployment files, remote management endpoints, TLS settings or credentials.

Do not expose the listener through a public proxy, SSH port forwarding or
Tailscale/Cloudflare tunnel. Authentication, authorization and encrypted
transport must be designed and tested before enabling remote access or
business-data endpoints.

## Verification

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p pos-server --locked
cargo build -p pos-server --locked
python3 scripts/server-smoke.py ./target/debug/pos-server
```

Rust integration tests use temporary loopback ports and verify routing,
configuration, occupied ports and shutdown. The Python smoke test starts
the **actual executable** on an ephemeral loopback port, checks JSON
responses, rejects exposed listen addresses, tests SIGTERM/SIGINT, and
verifies no POS user data was created or modified.

Future milestones: connect authenticated identity and PostgreSQL to secure
server APIs, establish workspace-scoped authorization and eventually synchronize.
The server now optionally uses PostgreSQL for authenticated user/workspace
metadata and workspace-scoped Tasks/Projects APIs. It still exposes no public
login, registration or remote-access support.
