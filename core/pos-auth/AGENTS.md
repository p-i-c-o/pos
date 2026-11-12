# pos-auth: crate-specific instructions

Read and follow the workspace root AGENTS.md. This is a library crate.

## Responsibility

User, session and device authentication primitives.

## Boundaries

Never expose credentials, session secrets or tokens through logs or debug output.

## Verification

Argon2id password verification and random bearer-token hashing are implemented
with unit tests. Expiry, device revocation, session revocation and disabled-user
checks are enforced by pos-postgres and tested using ephemeral CI PostgreSQL.
No HTTP login, self-registration or token exposure endpoint exists yet.
Do not log or serialize secrets through Debug; never expose private methods as
unauthenticated HTTP handlers.
