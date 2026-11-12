# POS - Personal Organisation System

> A fast, terminal-first, self-hosted platform for managing personal and organisational work in one in-house application.

<p align="center">
  <img alt="Status: Planning" src="https://img.shields.io/badge/Status-Planning-lightgrey">
  <img alt="Category: Software" src="https://img.shields.io/badge/Category-Software-0366d6">
  <img alt="Category: Self-Hosting" src="https://img.shields.io/badge/Category-Self--Hosting-1f6feb">
  <img alt="Version: 0.0.0" src="https://img.shields.io/badge/Version-0.0.0-orange">
  <img alt="Updated: 2026-11-12" src="https://img.shields.io/badge/Updated-2026--11--12-brightgreen">
</p>

**Status:** Architecture and planning. No working POS implementation or installation procedure exists yet.

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

**Not available yet.** This repository currently contains the project README and no runnable POS application.

1. Review the architecture and initial milestones below.
2. Start with the local TUI, tasks and projects prototype.
3. Add build and installation instructions here once working code exists.

Planned command forms (illustrative, **not implemented**):

```sh
pos
pos tasks
pos projects
pos tasks add "Review report" --project university
pos tasks list --json
```

## Repo Layout

The following is the **proposed**, not yet created, Rust workspace layout:

```text
pos/
├── client/       # TUI and CLI
├── core/         # Entities, commands, permissions, relationships, events
├── modules/      # Mail, tasks, projects, calendar, notes, Git, etc.
├── server/       # Network API, scheduler and workers
├── storage/      # PostgreSQL, SQLite, files and repositories
└── sync/         # Change tracking, queues and conflict handling
```

These are logical areas; actual Rust crates and package boundaries will be chosen during implementation.

## Docs

No separate documentation files have been created yet. Architectural decisions are summarised below; future detailed guides should cover:

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
| Terminal | Crossterm, minimal custom renderer |
| CLI | Clap |
| Server API | Axum |
| Databases | PostgreSQL (server), SQLite (client) |
| Database access | SQLx |
| Background work | Rust scheduler/worker and persistent database-backed jobs |
| Service supervision | systemd on Linux |

These are current design choices, not installed or benchmarked components. Performance goals include rapid startup, non-blocking navigation and low resource usage.

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

- **What works:** Architecture and initial implementation approach have been discussed and approved.
- **What doesn't (yet):** No source code, TUI, server, database schema or synchronisation is implemented.
- **Measurements / data:** No performance benchmarks or reliability measurements yet.
- **Media:** No screenshots or demos yet.

## Requirements

- **Hardware:** Development machine initially; self-hosted server and optional fallback hosts later.
- **Tools (planned):** Rust toolchain, Cargo, Git and a terminal; PostgreSQL for server development.
- **Software:** Cross-platform terminal client and POS server, to be developed.
- **Skills assumed:** Basic terminal/Git use for contributors; Rust and systems/database fundamentals for implementation.

## How to Contribute

POS is at the design stage. Use repository issues to propose changes or identify requirements. Before larger code contributions, agree on module boundaries, data models and security impact. Keep changes focused, documented and tested; never commit credentials.

## License

**Undecided.** No open-source license has been selected yet; do not assume a license from this README. Select and add a `LICENSE` file before inviting external code contributions.

## Credits

Inspired by the efficiency of traditional terminal clients such as irssi, WeeChat and mutt, and by the idea of a unified workspace. These are interface inspirations, not embedded dependencies.

## Contact

Project repository: https://github.com/p-i-c-o/pos
