# pos-jobs: crate-specific instructions

Read and follow the workspace root AGENTS.md. This is a library crate.

## Responsibility

Shared scheduling, queue, retry and job execution abstractions.

## Boundaries

Server workers run inside pos-server; avoid duplicate external side effects.

## Verification

Future tests should cover retries, idempotence, crashes and job recovery.
The current crate is a compiling scaffold only; do not imply that its future features exist.
