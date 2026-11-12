# pos-server: crate-specific instructions

Read and follow the workspace root AGENTS.md. This is a binary crate.

## Responsibility

The single pos-server executable and local Axum/Tokio HTTP runtime. Background workers remain unimplemented.

## Boundaries

No separate worker executable. All shared operations require authentication and authorisation.

## Verification

Tests cover startup, loopback-only binding, health routes, graceful SIGINT/SIGTERM shutdown and failure handling. Worker lifecycle is still future work.
The current server has no business APIs, authentication, database or synchronization; never expose it to remote clients.
