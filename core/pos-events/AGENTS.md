# pos-events: crate-specific instructions

Read and follow the workspace root AGENTS.md. This is a library crate.

## Responsibility

Internal domain-event definitions, publication and delivery abstractions.

## Boundaries

Preserve transactional outbox and idempotence requirements when persistence is added.

## Verification

Future tests should cover delivery ordering, duplication and failure recovery.
The current crate is a compiling scaffold only; do not imply that its future features exist.
