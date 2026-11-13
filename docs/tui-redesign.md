# POS TUI overview redesign

Status: implemented on the development feature branch, not included in
the stable v0.1.0 release. Based on the approved
`/root/projects/pos/ui-design/overview.txt` mockup. The reference file was
not modified.

## What changed

- Header shows local OS username and the host machine's local date/time
  (`YYYY/MM/DD | hh:mm am/pm`). It updates once per minute while POS runs.
  The version number moved to Settings > About.
- Sidebar defaults to Overview, Projects, Tasks, Calendar, Mail, Notes,
  Files, Contacts, Time, Chat, followed by a blank line and Settings.
- Overview and Settings never move. Other main-menu modules are sorted
  by how often they were opened over the preceding 7 days, with the default
  order preserved for ties. This ranking is calculated **at startup only**,
  so the sidebar cannot jump around during keyboard navigation.
- Settings opens a second-level sidebar with Back, a blank divider, Git,
  GitHub, GitLab, User Account, General and About. Back or Esc returns
  to the main menu. Entries that are not implemented say so explicitly.
- Right-panel titles change with context (Overview, Projects, Project /
  University, Tasks / All, Tasks / University, Settings / Git, and so on).
- Projects open into their own overview context. Each project presents a
  section menu for Overview, Tasks, Notes, Files, Calendar and Activity.
  Tasks is an explicitly selectable section, never an automatic redirect.
  Other project sections are clear placeholders, not pretend data storage.
  Esc or Back returns to the project list; the task filter still works when
  Tasks is deliberately opened.
- Workspace text and lists have extra horizontal padding.
- Exactly one `>` is visible at the active keyboard focus, on either the
  navigation row, a focused Task/Project row, or the help overlay.
  The selected module can remain highlighted after focus moves.
- Bottom shortcuts are grouped with visible separators and change
  according to focus (navigation, Tasks, Projects, Settings, editor, help).
- Overview now shows local open-task counts, overdue tasks, deadlines due
  within seven days, and available local project names. When a Tasks filter
  is active, its counts explicitly say `filtered view`.

Existing task/project keyboard operations, SQLite data, CLI commands,
domain validation and backend worker are unchanged. Other modules still
show accurate placeholders. In particular, Settings > User Account does
not provide server login yet.

## Local UI history

Only module-visit timestamp and module ID are stored in a small TSV file:

`$XDG_STATE_HOME/pos/navigation.tsv`

If `XDG_STATE_HOME` is not set, the path is
`~/.local/state/pos/navigation.tsv`. Example lines:

```
1794528000	2
1794528010	1
```

Each row records a successful navigation activation: when Enter or Tab
focuses a module, or a user explicitly opens the Tasks section of a project. Scrolling
through the sidebar alone does not count as a visit. No task content,
project names, user account data, passwords or tokens are stored.

The file is loaded when POS launches. Writes happen on a separate thread,
not in the keyboard event loop. Failed reads/writes do not prevent POS
from running. When the log grows beyond 64 KiB, the writer compacts it
to recent visits. There is no SQLite migration, no automated upload, and
no persistent business data format change.

Deleting this **UI history file only** resets navigation ranking to the
fixed default order on the next launch. It does not delete POS tasks,
projects or SQLite data. You don't need to delete it for normal operation.

## Verification

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build -p pos-cli --locked
```

Ratatui's test backend asserts the layout at exactly 100x32, including
navigation, Settings, contextual titles, Overview, project-specific
section navigation and the one-marker focus invariant. Separate tests cover sorting across the seven-day window,
stable tie ordering, persistent visit counting, invalid history entries,
menu movement and existing interactive task/project operations.

Live smoke checks can run with an isolated SQLite path and HOME:

```sh
ROOT="$(mktemp -d)"
HOME="$ROOT" XDG_STATE_HOME="$ROOT/state" ./target/debug/pos --db "$ROOT/preview.sqlite3"
```

Do not modify a real POS database to test layout changes. The UI runs on
the host's configured timezone; custom per-user timezone selection and
full server-connected dashboards remain future work.
