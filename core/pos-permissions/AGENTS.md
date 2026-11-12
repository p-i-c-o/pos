# pos-permissions: crate-specific instructions

Read and follow the workspace root AGENTS.md. This is a library crate.

## Responsibility

Workspace and nested-organisation access policy, RBAC and ABAC.

## Boundaries

Do not infer unrestricted access from organisational membership or managerial title.

## Verification

Tests cover deny-by-default access, distinct personal/shared roles, owner-only management and restrictions against implicit organisational privileges. The workspace role model currently covers owner, editor and viewer only. Nested organisations, role delegation and full RBAC/ABAC are not implemented.
