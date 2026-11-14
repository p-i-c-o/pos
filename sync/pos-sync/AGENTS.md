# pos-sync: crate-specific instructions

Read and follow the workspace root AGENTS.md. This is a library crate.

## Responsibility

Shared offline/online synchronisation, queued operations and conflict handling.

## Boundaries

Never silently overwrite conflicting edits or treat local queueing as server acknowledgement.

## Verification

Future tests should cover disconnected operation, duplication and divergent histories.
PR #18 uses pos-entities LocalRecordId and defines typed pending changes and a pure, conservative revision
comparison. It does not implement transport, server mapping,
acknowledgements, conflict resolution, persistence or remote uploads.
Never infer successful sync from queued events. See docs/sync-foundations.md.

## PR #20 server-side protocol

Server PostgreSQL now stores a transactional incremental change feed and
per-device operation receipts, with authenticated GET/POST loopback endpoints.
Client queue draining, automatic upload/download and delete/tombstone handling
are not implemented. See docs/server-sync-protocol.md.
