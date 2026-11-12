# pos-core: crate-specific instructions

Read and follow the workspace root AGENTS.md. This is a library crate.

## Responsibility

Shared POS application operations and foundational business interfaces.

## Boundaries

Avoid module-specific persistence, terminal rendering or direct external provider logic.

## Verification

Future tests should cover operation validation and consistent error handling.
The current crate is a compiling scaffold only; do not imply that its future features exist.
