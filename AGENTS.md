# POS Development Instructions

These instructions apply to the entire POS repository. Read `README.md` for the approved product vision, architecture and planned milestones. This document governs how coding agents work; it does not authorize work beyond the user's request.

## 1. Purpose and priorities

- POS is an in-house, terminal-first, self-hosted personal and organisational system.
- Optimise for correctness, reliability, low latency, low resource use and keyboard efficiency before visual polish.
- Keep the TUI dense and functional, inspired by classic Unix terminal clients.
- Maintain an ordinary, useful CLI alongside the TUI; make operations scriptable.
- Keep the product usable without AI or third-party productivity services.

## 2. Architecture boundaries

- Build one modular monolith, not a network of microservices or third-party application wrappers.
- Use shared entities, relationships, commands/queries, permission checks and events across native modules.
- Each module owns its business rules and detailed data; avoid direct cross-module database manipulation.
- Preserve offline-first and online-first modes, durable outgoing queues and safe sync conflict handling.
- Preserve one authoritative writer for shared state; never allow unsafe failover or silent data loss.
- Store native Git repositories as Git; support GitHub and GitLab as optional external sources.
- Keep personal data isolated from shared workspace data; organisational membership and managerial authority never imply unrestricted access.
- Do not build speculative infrastructure for features that are not yet being implemented.

## 3. Technical direction

- The planned stack is Rust, a lightweight Crossterm-based TUI, Clap CLI, Axum server, PostgreSQL server storage and SQLite local storage.
- This is a documented design direction, not permission to add dependencies or create the entire stack at once.
- Prefer simple, maintainable, idiomatic Rust and lightweight established libraries.
- Keep data model, storage and UI concerns separated; do not block keyboard input on network operations.
- Avoid global mutable state, hidden side effects and unnecessary abstractions.

### Approved workspace structure

- All 27 crates belong to one Rust workspace, grouped under client/, server/, core/, storage/, sync/ and modules/.
- The only executables are pos (from pos-cli) and pos-server. pos-tui is a library used by pos-cli.
- pos-cli and pos-server reuse pos-core; do not duplicate shared application logic.
- pos-entities owns shared IDs/relationships; pos-events owns domain events; pos-protocol owns client/server wire types.
- pos-auth owns authentication; pos-permissions owns scoped authorisation; pos-jobs owns common job abstractions.
- pos-storage defines common storage interfaces; pos-sqlite and pos-postgres implement the respective backends.
- pos-sync implements shared synchronisation rules. Feature crates own their domain-specific behaviour.
- Notes and journal share pos-notes. Native Git, GitHub and GitLab have separate crates.
- pos-mail is initially an IMAP/SMTP client only. AI and advanced automation crates are not in the scaffold.
- Read the nearest crate AGENTS.md before modifying that crate. Root instructions always apply.
- The initial implementation is intentionally a compiling scaffold. Do not invent implementation behind placeholders.
- Rust edition 2024 and Rust 1.99.1 are pinned in the workspace; use Cargo and formatting/linting consistently.

## 4. Explicit approval gates

**Stop and request specific user approval before:**

- Changing approved architecture, core technology choices or established module boundaries.
- Introducing, removing or substantially upgrading dependencies.
- Creating or modifying database schemas, migrations or persistent data formats.
- Creating, removing or changing public APIs, CLI contracts or on-disk/protocol compatibility.
- Making significant changes to authentication, authorisation, encryption, secrets handling or permission policies.
- Deleting, overwriting, migrating or bulk-editing existing user data; performing destructive commands or disruptive infrastructure operations.
- Merging branches, pushing to any remote or publishing releases.

Do not infer approval from silence, previous unrelated approvals or the presence of an item in the roadmap. An explicit user request approving that particular change counts. If a task crosses a gate, explain the proposed change, impact and alternatives briefly and wait for approval. Do not quietly implement the gated portion.

## 5. Allowed autonomous work

Within explicitly approved scope, agents may:

- Implement requested functionality using already approved dependencies, schema and interfaces.
- Make local, behaviour-preserving refactors and focused bug fixes.
- Write tests, run non-destructive verification and update relevant documentation.
- Create working branches and local commits when appropriate; do not merge or push without approval.

Do not expand the task into unrelated features or modify files outside the repository without explicit authorisation. Preserve unrelated user changes and keep the working tree understandable.

## 6. Security and data safety

- Enforce access control on the server and through the shared command path, including AI and automation actions.
- Use least privilege and scoped credentials; never hardcode, print, commit or expose secrets.
- Treat external input, mail, Git content and plugin output as untrusted data.
- Never silently overwrite conflicting offline edits or drop queued operations.
- Make background jobs and retries safe against duplicate processing wherever possible.
- Provide a backup and recovery plan before risky changes to persisted data.

## 7. Testing and performance

- Add tests for new behaviour, permissions, failures and edge cases; test offline and sync behaviour when relevant.
- Once the Rust workspace exists, run applicable `cargo fmt --check`, `cargo clippy` and `cargo test` commands.
- Benchmark performance-sensitive changes rather than asserting that the TUI is fast.
- Verify outcomes before reporting success; distinguish working code from plans or untested assumptions.
- If a check cannot run, state why and what remains unverified.

## 8. Git, documentation and reporting

- Start new work on a focused branch; do not push directly to `main`.
- Keep commits narrowly scoped with descriptive messages. Never commit generated secrets, caches or credentials.
- Update README or other relevant docs when implemented behaviour, commands or architecture change.
- At completion, report: changes made, files affected, tests and results, open issues, and any approvals needed.
- Keep this root file concise. Add module-specific `AGENTS.md` files only when those modules exist and need distinct rules.
