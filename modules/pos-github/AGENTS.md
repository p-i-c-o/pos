# pos-github: crate-specific instructions

Read and follow the workspace root AGENTS.md. This is a library crate.

## Responsibility

Optional GitHub provider integration for repositories, reviews and related metadata.

## Boundaries

Do not make GitHub mandatory or replace native repository ownership; protect access tokens.

## Verification

Future tests should cover provider errors, pagination and rate limits.
The current crate is a compiling scaffold only; do not imply that its future features exist.
