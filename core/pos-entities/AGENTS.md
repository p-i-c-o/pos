# pos-entities: crate-specific instructions

Read and follow the workspace root AGENTS.md. This is a library crate.

## Responsibility

Cross-module entity identifiers, references and relationship types.

## Boundaries

Feature crates retain ownership of detailed domain models; prevent ambiguous or cross-scope links.

## Verification

PR #18 introduces a typed local UUID ID, which cannot be passed where
a raw server UUID is expected. The format serializes transparently to
a UUID string; remote identity mapping and workspace scoping remain future
work. Tests verify parsing and serialization.
