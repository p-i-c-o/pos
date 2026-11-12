# pos-protocol: crate-specific instructions

Read and follow the workspace root AGENTS.md. This is a library crate.

## Responsibility

Shared client/server request, response and compatibility types.

## Boundaries

Keep transport and business implementation outside shared wire contracts; version changes deliberately.

## Verification

Future tests should cover serialization and backward compatibility.
The current crate is a compiling scaffold only; do not imply that its future features exist.
