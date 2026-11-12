# pos-permissions: crate-specific instructions

Read and follow the workspace root AGENTS.md. This is a library crate.

## Responsibility

Workspace and nested-organisation access policy, RBAC and ABAC.

## Boundaries

Do not infer unrestricted access from organisational membership or managerial title.

## Verification

Future tests should cover denial by default, delegation bounds and cross-workspace isolation.
The current crate is a compiling scaffold only; do not imply that its future features exist.
