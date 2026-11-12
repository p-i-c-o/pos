# pos-sync: crate-specific instructions

Read and follow the workspace root AGENTS.md. This is a library crate.

## Responsibility

Shared offline/online synchronisation, queued operations and conflict handling.

## Boundaries

Never silently overwrite conflicting edits or treat local queueing as server acknowledgement.

## Verification

Future tests should cover disconnected operation, duplication and divergent histories.
The current crate is a compiling scaffold only; do not imply that its future features exist.
