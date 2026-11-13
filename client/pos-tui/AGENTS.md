# pos-tui: crate-specific instructions

Read and follow the workspace root AGENTS.md. This is a library crate.

## Responsibility

Minimal, fast terminal presentation, keyboard navigation and client interaction.

## Boundaries

Keep storage, network connections and business rules out of rendering. Do not block input on I/O.

## Verification

Tests cover navigation, focus-marker uniqueness, 100x32 rendering,
Settings/Back behavior, dedicated project overview (separate from Tasks),
project section navigation, seven-day usage ranking and local Tasks/Projects input.
The root CLI continues to own SQLite and its worker.
The only TUI-specific disk state is module-visit timestamps under
XDG_STATE_HOME/pos/navigation.tsv (fallback ~/.local/state/pos).
Only read this history at startup and write events on a separate thread.
Never store task/project content or credentials in UI history.
Keep placeholders separate from implemented modules and do not block keyboard input on I/O.
