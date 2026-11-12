# pos-calendar: crate-specific instructions

Read and follow the workspace root AGENTS.md. This is a library crate.

## Responsibility

Events, attendees, recurrence and scheduling domain logic.

## Boundaries

Keep timezone and recurrence semantics explicit; storage and notification are separate layers.

## Verification

Future tests should cover DST transitions and recurring event changes.
The current crate is a compiling scaffold only; do not imply that its future features exist.
