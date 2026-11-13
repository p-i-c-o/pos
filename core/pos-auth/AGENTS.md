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
PR #17 adds development-only loopback HTTP password login (returning a
one-time session token) and bearer-authenticated logout. No public
registration or internet-facing login exists. Never log credentials or
serialize secrets via Debug; preserve rate limits and revocation checks.
