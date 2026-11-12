# pos-server: crate-specific instructions

Read and follow the workspace root AGENTS.md. This is a binary crate.

## Responsibility

The single pos-server executable and local Axum/Tokio HTTP runtime. Background workers remain unimplemented.

## Boundaries

No separate worker executable. All shared operations require authentication and authorisation.

## Verification

Tests cover startup, loopback-only binding, health routes, graceful shutdown and error handling. Optional PostgreSQL-backed GET-only identity and scoped workspace metadata endpoints validate active bearer sessions on each request. The server has no public login/registration, Tasks/Projects data APIs, worker jobs, remote access or sync. Never expose it to remote clients.
