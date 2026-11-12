# pos-projects: crate-specific instructions

Read and follow the workspace root AGENTS.md. This is a library crate.

## Responsibility

Project organisation and links to related POS entities.

## Boundaries

Use shared entity references instead of copying or owning records from other modules.

## Verification

Future tests should cover membership, access and cross-module links.
Project validation, naming and stable slug generation are implemented. Membership and authorisation are not.
