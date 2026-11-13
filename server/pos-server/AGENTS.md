# pos-server: crate-specific instructions

Read and follow the workspace root AGENTS.md. This is a binary crate.

## Responsibility

The single pos-server executable and local Axum/Tokio HTTP runtime. Background workers remain unimplemented.

## Boundaries

No separate worker executable. All shared operations require authentication and authorisation.

## Verification

Tests cover startup, loopback-only binding, health routes, graceful shutdown and error handling. Optional PostgreSQL-backed identity/workspace metadata and server Tasks/Projects create/read/update endpoints validate active bearer sessions on each request. Workspace access is rechecked at the repository boundary. The server has no public login/registration, hard-delete routes, worker jobs, remote access or sync. Never expose it to remote clients.

## Development-only onboarding

The server CLI now offers trusted local `admin create-user`,
`revoke-device` and `disable-user`, requiring POS server database
credentials. User and Personal workspace creation is atomic. Loopback HTTP
supports bounded rate-limited password login and bearer self-logout;
no public registration or external exposure. Never log secrets.
See docs/account-onboarding.md.
