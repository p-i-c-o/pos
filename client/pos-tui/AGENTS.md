# pos-tui: crate-specific instructions

Read and follow the workspace root AGENTS.md. This is a library crate.

## Responsibility

Minimal, fast terminal presentation, keyboard navigation and client interaction.

## Boundaries

Keep storage, network connections and business rules out of rendering. Do not block input on I/O.

## Verification

Future tests should cover navigation, rendering and terminal cleanup.
The Ratatui terminal shell is implemented. All operational modules remain placeholders; do not imply they work.
