# pos-server: crate-specific instructions

Read and follow the workspace root AGENTS.md. This is a binary crate.

## Responsibility

The single pos-server executable, server runtime and integrated background workers.

## Boundaries

No separate worker executable. All shared operations require authentication and authorisation.

## Verification

Future tests should cover startup, graceful shutdown and worker lifecycle.
The current crate is a compiling scaffold only; do not imply that its future features exist.
