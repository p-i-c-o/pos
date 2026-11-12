# pos-core: crate-specific instructions

Read and follow the workspace root AGENTS.md. This is a library crate.

## Responsibility

Shared POS application operations and foundational business interfaces.

## Boundaries

Avoid module-specific persistence, terminal rendering or direct external provider logic.

## Verification

Future tests should cover operation validation and consistent error handling.
TaskService and ProjectService implement validated shared operations, including project task links. No UI or SQL belongs here.
