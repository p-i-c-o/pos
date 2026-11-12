# pos-git: crate-specific instructions

Read and follow the workspace root AGENTS.md. This is a library crate.

## Responsibility

Native Git repository integration and Git-specific domain operations.

## Boundaries

Use Git semantics and tools rather than reimplementing Git storage; avoid destructive repo actions.

## Verification

Future tests should cover branch safety and working-tree preservation.
The current crate is a compiling scaffold only; do not imply that its future features exist.
