# pos-auth: crate-specific instructions

Read and follow the workspace root AGENTS.md. This is a library crate.

## Responsibility

User, session and device authentication primitives.

## Boundaries

Never expose credentials, session secrets or tokens through logs or debug output.

## Verification

Future tests should cover expired, revoked and malformed credentials.
The current crate is a compiling scaffold only; do not imply that its future features exist.
