# POS - Personal Organisation System

> A fast, terminal-first, self-hosted platform for managing personal and organisational work in one in-house application.

<p align="center">
  <!-- STATUS BADGES: enable one appropriate status -->
  <!-- <img alt="Status: Planning" src="https://img.shields.io/badge/Status-Planning-lightgrey"> -->
  <img alt="Status: In Progress" src="https://img.shields.io/badge/Status-In%20Progress-blue">
  <!-- <img alt="Status: Prototype" src="https://img.shields.io/badge/Status-Prototype-blueviolet"> -->
  <!-- <img alt="Status: Beta" src="https://img.shields.io/badge/Status-Beta-yellow"> -->
  <!-- <img alt="Status: Stable" src="https://img.shields.io/badge/Status-Stable-brightgreen"> -->
  <!-- <img alt="Status: Completed" src="https://img.shields.io/badge/Status-Completed-brightgreen"> -->
  <!-- <img alt="Status: Completed, docs WIP" src="https://img.shields.io/badge/Status-Completed%2C%20docs%20WIP-blue"> -->
  <!-- <img alt="Status: Maintenance" src="https://img.shields.io/badge/Status-Maintenance-informational"> -->
  <!-- <img alt="Status: Archived" src="https://img.shields.io/badge/Status-Archived-9f9f9f"> -->
  <!-- <img alt="Status: On Hold" src="https://img.shields.io/badge/Status-On%20Hold-orange"> -->
  <!-- <img alt="Status: Deprecated" src="https://img.shields.io/badge/Status-Deprecated-red"> -->
  <!-- <img alt="Status: Broken" src="https://img.shields.io/badge/Status-Broken-critical"> -->
  <!-- <img alt="Status: Experimental" src="https://img.shields.io/badge/Status-Experimental-purple"> -->
  <!-- CATEGORY BADGES: enable relevant categories -->
  <!-- <img alt="Category: Hardware" src="https://img.shields.io/badge/Category-Hardware-6f42c1"> -->
  <img alt="Category: Software" src="https://img.shields.io/badge/Category-Software-0366d6">
  <!-- <img alt="Category: Firmware" src="https://img.shields.io/badge/Category-Firmware-0e8a16"> -->
  <!-- <img alt="Category: Electronics" src="https://img.shields.io/badge/Category-Electronics-ff9500"> -->
  <!-- <img alt="Category: IoT" src="https://img.shields.io/badge/Category-IoT-00bcd4"> -->
  <!-- <img alt="Category: Home Assistant" src="https://img.shields.io/badge/Category-Home%20Assistant-41bdf5"> -->
  <!-- <img alt="Category: ESPHome" src="https://img.shields.io/badge/Category-ESPHome-000000"> -->
  <!-- <img alt="Category: 3D Printing" src="https://img.shields.io/badge/Category-3D%20Printing-8a2be2"> -->
  <!-- <img alt="Category: CAD" src="https://img.shields.io/badge/Category-CAD-4b5563"> -->
  <!-- <img alt="Category: Networking" src="https://img.shields.io/badge/Category-Networking-2ea44f"> -->
  <!-- <img alt="Category: DevOps" src="https://img.shields.io/badge/Category-DevOps-24292e"> -->
  <img alt="Category: Self-Hosting" src="https://img.shields.io/badge/Category-Self--Hosting-1f6feb">
  <!-- <img alt="Category: Mixed" src="https://img.shields.io/badge/Category-Mixed-lightgrey"> -->
  <!-- <img alt="Category: Research" src="https://img.shields.io/badge/Category-Research-7c3aed"> -->
  <!-- <img alt="Category: School" src="https://img.shields.io/badge/Category-School-ff69b4"> -->
  <!-- META BADGES: update active version/date values as needed -->
  <img alt="Version: 0.0.0" src="https://img.shields.io/badge/Version-0.0.0-orange">
  <img alt="Updated: 2026-11-12" src="https://img.shields.io/badge/Updated-2026--11--12-brightgreen">
  <!-- <img alt="Version" src="https://img.shields.io/badge/Version-x.x.x-orange"> -->
  <!-- <img alt="Updated" src="https://img.shields.io/badge/Updated-YYYY--MM--DD-brightgreen"> -->
</p>

**Status:** Early local prototype. Tasks and Projects have functional shared operations, SQLite persistence, CLI commands and Ratatui panels. The server and other modules remain placeholders.

## Overview

- **Goal:** Build a unified, extensible personal organisation system that runs primarily in a terminal, supports individuals and organisations, and remains usable with limited connectivity.
- **Why:** Keep mail, tasks, projects, notes, files, scheduling, development and communication in one coherent product instead of stitching together many productivity applications.
- **Scope:** Native POS modules, shared entities and permissions, a lightweight TUI and scriptable CLI, a self-hosted server, configurable offline-first or online-first operation, synchronisation, backups and optional external protocols/providers where appropriate.
- **Out of scope:** A web-first dashboard, microservice sprawl, dependence on third-party productivity suites for core features, and shipping every module in the first prototype.

POS is intended to be **written in-house**. PostgreSQL, SQLite and Git are implementation technologies; POS modules are not wrappers around separate productivity applications. External GitHub/GitLab repositories and existing email providers remain optional choices.

## Highlights

- **Terminal-first:** Dense, keyboard-driven interfaces inspired by traditional Unix mail and chat clients, plus automation-friendly shell commands.
- **One coherent data model:** Tasks, projects, messages, files, people, events and repositories can be linked without copying their data.
- **Offline-first or online-first:** Choose a preferred mode per deployment/workspace; queue offline changes and synchronise later.
- **Reliable self-hosting:** Primary and fallback servers, durable jobs, conflict handling, version history and tested backups.
- **Flexible organisations:** Arbitrarily nested organisational units, multiple memberships, scoped roles and delegated administration.
- **Git flexibility:** Native repository hosting or GitHub/GitLab (including self-hosted GitLab) as the authoritative source per repository.
- **One command system:** TUI, CLI, automation and future AI use the same validated, permission-checked operations.

### Planned modules

| Module | Responsibility |
| --- | --- |
| Mail | Inbox, threads, composition, folders/tags, links to projects; existing IMAP/SMTP accounts first, optional native mail hosting later |
| Calendar | Events, attendees, recurrence, files and conferencing details |
| Tasks | Lists, deadlines, priorities, recurrence, assignments and related records |
| Projects | Central context for tasks, messages, notes, files, people, time and code |
| Notes and journal | Searchable notes, backlinks and private chronological entries |
| Files and contacts | File storage, metadata, versioning and people/organisation records |
| Git | Repositories, commits, branches, issues and provider-specific review/CI details |
| Time | Explicit project time tracking, not silently running in the background |
| Chat | Native direct messages, channels and project-linked threads (later) |
| Search, automation and AI | Cross-module search, durable workflows and scoped assistance (later) |

## Quick Start

**Working:** Local Tasks and Projects with SQLite via SQLx, shared service operations, a scriptable Clap CLI and Ratatui UI. The server, multiuser permissions and cross-device synchronisation are **not implemented**.

Install Rust via rustup if needed; this repository pins Rust 1.99.1.

```sh
cargo build --locked -p pos-cli
./target/debug/pos                            # Open the interactive TUI
./target/debug/pos projects add "University"  # Generates slug "university"
./target/debug/pos projects list --all --json
./target/debug/pos projects edit 1 --name "University Work"
./target/debug/pos projects archive 1
./target/debug/pos projects unarchive 1
./target/debug/pos tasks add "Write report" --project university --due 2026-12-17
./target/debug/pos tasks list --project university
./target/debug/pos tasks edit 1 --project university
./target/debug/pos tasks edit 1 --clear-project
./target/debug/pos tasks list --all --json
./target/debug/pos tasks done 1
./target/debug/pos tasks reopen 1
./target/debug/pos tasks delete 1 --yes       # Permanently deletes one task
./target/debug/pos projects delete 1 --yes    # Deletes project, preserves its tasks
```

Projects use a **stable unique slug** (for example, `university-work`) which does not change when you rename a project. You can specify `projects add "University" --slug university` to choose it explicitly. By default, lists hide archived projects and completed tasks. `--all` includes them.

**Data location:** `$XDG_DATA_HOME/pos/pos.sqlite3`, or `~/.local/share/pos/pos.sqlite3` if XDG_DATA_HOME is unset. Override it with `--db /path/to/pos.sqlite3` or `POS_DB_PATH=/path/to/pos.sqlite3`. SQLx runs additive, versioned migrations, including on existing Phase 3 task databases. The new task-project relationship is nullable, so existing tasks remain unassigned. When a project is deleted, its tasks are **not deleted**. Project and task deletion require `--yes` at the CLI and interactive confirmation in Ratatui. Back up your SQLite database before intentionally deleting real data.

**Ratatui:** Use `j/k` or arrows to select **Projects** or **Tasks**, then `Enter` to focus. Projects: `a` add, `e` rename, `Space` archive/restore, `x` delete with confirmation, `Enter` open that project's filtered tasks. Tasks: `a` add, `e` rename, `d` due date, `p` assign by project slug, `u` unassign, `f` change the project filter (empty input clears it), `Space` toggle completion and `x` delete with confirmation. `Esc` backs out, `?` shows help, `r` refreshes, and `q` quits. Data access happens in a worker separate from the event loop.

Projects currently provide **one-to-many task grouping only**. Project membership, permissions, task priorities, recurrence, subprojects, calendar integration and server sync are not implemented.

### Development verification

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --locked
```

Future features include multiuser permissions, calendar scheduling and server synchronisation.

## Repo Layout

The repository contains one Cargo workspace with **27 Rust crates**, organised by responsibility. Each crate has its own Cargo.toml, src/ and AGENTS.md.

```text
pos/
├── AGENTS.md
├── README.md
├── Cargo.toml
├── Cargo.lock
├── rust-toolchain.toml
├── rustfmt.toml
├── .gitignore
├── .github/workflows/ci.yml
├── client/
│   ├── pos-cli/          # pos executable
│   └── pos-tui/          # Terminal UI library
├── server/
│   └── pos-server/       # pos-server executable and future workers
├── core/
│   ├── pos-core/
│   ├── pos-entities/
│   ├── pos-events/
│   ├── pos-protocol/
│   ├── pos-auth/
│   ├── pos-permissions/
│   └── pos-jobs/
├── storage/
│   ├── pos-storage/
│   ├── pos-sqlite/
│   └── pos-postgres/
├── sync/
│   └── pos-sync/
└── modules/
    ├── pos-mail/
    ├── pos-calendar/
    ├── pos-tasks/
    ├── pos-projects/
    ├── pos-notes/
    ├── pos-files/
    ├── pos-contacts/
    ├── pos-time/
    ├── pos-chat/
    ├── pos-search/
    ├── pos-git/
    ├── pos-github/
    └── pos-gitlab/
```

The pos-tui crate implements the interactive Ratatui shell; most crates remain compiling placeholders. AI and advanced automation crates are intentionally absent.

## Docs

Root and per-crate AGENTS.md files document development boundaries. Architectural decisions are summarised below; future detailed guides should cover:

- **How it works:** Core entities, module boundaries, relationships and events.
- **Build / Installation:** Development, compilation, server deployment and client installation.
- **Configuration:** Operating mode, storage scope, keybindings, providers and secrets.
- **Testing / Validation:** Offline conflict tests, permission tests, failover and restore drills.
- **Troubleshooting:** Sync errors, queued operations, jobs and connectivity.

## Notes / Design Log

### High-level architecture

```text
               POS TUI / CLI
                     |
            Shared command/query API
                     |
        POS core: identity, permissions,
        entities, search, links and events
                     |
        Native modules (tasks, mail, ...)
                     |
           Data and sync layer
                /        \
         Local SQLite   Server PostgreSQL
                \        /
             Files and Git
```

- **Modular monolith:** One POS product and codebase, with clear internal module boundaries; avoid separate service stacks for built-in functionality.
- **Storage:** PostgreSQL on the server, SQLite on offline-capable clients, normal files for attachments and native Git repositories. The server is authoritative for shared state.
- **Shared entities:** Common IDs and a relationships registry link domain-specific records. Each module owns the detailed data and enforces permissions.
- **Internal communication:** Direct requests for immediate answers; durable events for reactions. Persist business changes and their events atomically (transactional outbox).
- **Unified operations:** Commands change state; queries read state. CLI, TUI, automations and AI share validation, permission checks, audit records and consistent errors.
- **Sync:** Local edits and outgoing messages use durable queues. Distinguish local acceptance from server confirmation; use unique operation IDs, version checks and conflict preservation. Outbound email retries need special duplicate-send safeguards.
- **Availability:** Clients may use fallback servers. A fallback is read-only unless exclusive write ownership can be safely established, preventing split-brain.
- **Identity:** A user can belong to personal and shared workspaces. Organisations have unlimited nested administrative units and cross-functional memberships; roles are scoped and delegation cannot grant more authority than the delegator has.
- **Security:** Authenticated/encrypted connections, per-device credentials, local encryption, server-side authorisation and auditable sensitive operations. Offline copies cannot be reliably revoked retroactively.
- **Backups:** Replication, snapshots/version history and backups are distinct safeguards. Backups must cover databases, files, native repositories, configuration and recovery material, with offsite copies and restore testing.
- **Extension model:** Configuration and scriptable commands first; restricted plugins and native automation later. Core functionality must not depend on plugins.
- **Interfaces:** Fast, minimal, keyboard-centric TUI over SSH or locally. Optional web/mobile interfaces later, all using the same core/protocol.

### Proposed technology stack

| Layer | Candidate |
| --- | --- |
| Main implementation | Rust workspace |
| Terminal | Ratatui 0.30.2 with Crossterm 0.29 backend (initial shell implemented) |
| CLI | Clap |
| Server API | Axum |
| Databases | PostgreSQL (server), SQLite (client) |
| Database access | SQLx |
| Background work | Rust scheduler/worker and persistent database-backed jobs |
| Service supervision | systemd on Linux |

Ratatui, Clap, SQLite and SQLx are used for local Tasks and Projects. Server components remain planned. Performance goals include rapid startup, non-blocking navigation and low resource usage; no formal benchmarks exist yet.

## Roadmap

- [ ] **v0.1 - Local prototype:** TUI, CLI, local SQLite, tasks and projects.
- [ ] **v0.2 - Connectivity:** Server API, accounts, device identity and basic synchronisation.
- [ ] **v0.3 - Shared information:** Notes, files, entity relationships and universal search.
- [ ] **v0.4 - Planning:** Calendar, reminders and background jobs.
- [ ] **v0.5 - Communication:** Mail client and contacts.
- [ ] **v0.6 - Development:** Native Git plus GitHub/GitLab backends.
- [ ] **v0.7 - Collaboration:** Organisational trees, scoped administration and multiuser features.
- [ ] **v0.8 - Messaging and workflows:** Chat and automations.
- [ ] **v0.9 - Advanced capabilities:** AI interface and extensions.
- [ ] **v1.0 - Stable release:** Hardening, reliable backup/restore, offline robustness and supported deployments.

Security boundaries, data portability and multiuser-aware modelling must be considered from the start, even if their full interfaces arrive later. Milestones are plans, not release dates.

## Results

- **What works:** 27-crate workspace; persistent local Tasks and Projects, task-project linking and filtered views, CLI JSON output, SQLx migrations, Ratatui management, tests and CI.
- **What doesn't (yet):** Other functional modules, a server listener, authentication, permissions, multiuser data isolation, project membership, priorities, recurrence and cross-device sync.
- **Measurements / data:** No performance benchmarks or reliability measurements yet.
- **Media:** No screenshots or demos yet.

## Requirements

- **Hardware:** Development machine initially; self-hosted server and optional fallback hosts later.
- **Tools:** Rust 1.99.1 (pinned by rust-toolchain.toml), Cargo, Git and a terminal. PostgreSQL will be needed for future server development.
- **Software:** Interactive local POS Tasks and Projects plus a placeholder server; no installation or production multiuser deployment is supported.
- **Skills assumed:** Basic terminal/Git use for contributors; Rust and systems/database fundamentals for implementation.

## How to Contribute

POS is in early implementation. Use repository issues to propose changes or identify requirements. Before larger code contributions, agree on module boundaries, data models and security impact. Keep changes focused, documented and tested; never commit credentials.

## License

**Undecided.** No open-source license has been selected yet; do not assume a license from this README. Select and add a `LICENSE` file before inviting external code contributions.

## Credits

Inspired by the efficiency of traditional terminal clients such as irssi, WeeChat and mutt, and by the idea of a unified workspace. These are interface inspirations, not embedded dependencies.

## Contact

Project repository: https://github.com/p-i-c-o/pos
