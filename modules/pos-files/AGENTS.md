# pos-files: crate-specific instructions

Read and follow the workspace root AGENTS.md. This is a library crate.

## Responsibility

File metadata, attachments, ownership and version references.

## Boundaries

Never trust supplied paths or filenames. Avoid traversal and unapproved destructive overwrites.

## Verification

Future tests should cover path safety, isolation and content integrity.
The current crate is a compiling scaffold only; do not imply that its future features exist.
