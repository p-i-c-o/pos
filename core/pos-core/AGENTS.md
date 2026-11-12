# pos-core: crate-specific instructions

Read and follow the workspace root AGENTS.md. This is a library crate.

## Responsibility

Shared POS application operations and foundational business interfaces.

## Boundaries

Avoid module-specific persistence, terminal rendering or direct external provider logic.

## Verification

Future tests should cover operation validation and consistent error handling.
TaskService provides validated atomic TaskEdit and TaskSearch operations, ProjectService handles projects, and versioned LocalDataExport exposes data. No UI or SQL belongs here.
