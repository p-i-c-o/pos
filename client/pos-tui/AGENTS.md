# pos-tui: crate-specific instructions

Read and follow the workspace root AGENTS.md. This is a library crate.

## Responsibility

Minimal, fast terminal presentation, keyboard navigation and client interaction.

## Boundaries

Keep storage, network connections and business rules out of rendering. Do not block input on I/O.

## Verification

Future tests should cover navigation, rendering and terminal cleanup.
The Tasks panel is interactive. Send requests to the nonblocking CLI-owned worker and render responses; other modules remain placeholders.
