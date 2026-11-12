# pos-entities: crate-specific instructions

Read and follow the workspace root AGENTS.md. This is a library crate.

## Responsibility

Cross-module entity identifiers, references and relationship types.

## Boundaries

Feature crates retain ownership of detailed domain models; prevent ambiguous or cross-scope links.

## Verification

Future tests should cover ID uniqueness, link validity and scope boundaries.
The current crate is a compiling scaffold only; do not imply that its future features exist.
