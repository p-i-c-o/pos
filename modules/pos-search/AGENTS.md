# pos-search: crate-specific instructions

Read and follow the workspace root AGENTS.md. This is a library crate.

## Responsibility

Unified cross-module search abstractions and results.

## Boundaries

Always apply permissions before returning snippets or metadata, including offline copies.

## Verification

Future tests should cover indexing consistency and private-result isolation.
The current crate is a compiling scaffold only; do not imply that its future features exist.
