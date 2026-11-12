# pos-gitlab: crate-specific instructions

Read and follow the workspace root AGENTS.md. This is a library crate.

## Responsibility

Optional GitLab provider integration, including self-hosted instances.

## Boundaries

Do not make GitLab mandatory; respect configured instance boundaries and protect tokens.

## Verification

Future tests should cover instance URLs, pagination and authentication errors.
The current crate is a compiling scaffold only; do not imply that its future features exist.
