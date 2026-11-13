# Phase 9d - Authenticated server Tasks and Projects

This is an experimental development-only API. It stores server records in
PostgreSQL and has **no synchronization or implicit relationship** to the
local SQLite database or the v0.1.0 laptop client.

## Starting the server

Without `POS_SERVER_DATABASE_URL`, the server remains health-only. With that
environment variable explicitly set to a **dedicated POS PostgreSQL database**,
the server applies pending SQLx migrations and offers authenticated API
operations. The loopback-only binding remains mandatory.

**Migration warning:** The new migration creates `pos_projects` and
`pos_tasks`. Migrations modify the chosen database and are not automatically
reversible. Back up an existing POS database before enabling this feature.
Never point it at an unrelated or production database during experiments.
No PostgreSQL service or user data is created on the homelab by this change.

At the original Phase 9d there was no login route. PR #17 now adds
restricted loopback HTTP login/logout and trusted CLI account provisioning;
see [onboarding](account-onboarding.md). Do not expose the server publicly.

## Versioned HTTP API

Replace `{ws}` with a workspace UUID. All routes require a single valid
`Authorization: Bearer <token>` header. User identity comes only from
PostgreSQL's active device-bound session, not client-supplied JSON.

| Method | Path | Operation |
| --- | --- | --- |
| GET | `/api/v1/workspaces/{ws}/projects` | List visible projects |
| POST | `/api/v1/workspaces/{ws}/projects` | Create a project |
| GET | `/api/v1/workspaces/{ws}/projects/{id}` | Read one project |
| PATCH | `/api/v1/workspaces/{ws}/projects/{id}` | Rename/archive/unarchive |
| GET | `/api/v1/workspaces/{ws}/tasks` | List visible tasks |
| POST | `/api/v1/workspaces/{ws}/tasks` | Create a task |
| GET | `/api/v1/workspaces/{ws}/tasks/{id}` | Read one task |
| PATCH | `/api/v1/workspaces/{ws}/tasks/{id}` | Edit or complete task |

There is **no DELETE**, workspace administration or client synchronization.
List endpoints are limited to the first **200 records** by creation date;
pagination and filtering are not yet implemented. Do not rely on list endpoints
for full backup or export.

### Example payloads

Create a project:

```json
{"name":"University coursework","slug":"uni-coursework"}
```

`slug` is optional. The domain module generates a stable slug when absent.
Slugs must be unique within each workspace, but can repeat in different ones.

Create a task:

```json
{
  "title":"Write the coursework report",
  "description":"Finish first draft",
  "priority":"high",
  "status":"todo",
  "due_date":"2026-12-17",
  "project_id":null
}
```

Only `title` is required. Priorities are `low`, `normal`, `high` or
`urgent`. Status values are `todo`, `in-progress`, `blocked` and
`completed`. A completed status automatically implies `completed: true`
in API responses; the flag is never stored separately. Due dates use
validated `YYYY-MM-DD` calendar dates.

Update a project:

```json
{"expected_version":1,"name":"University","archived":true}
```

Update a task, completing it and clearing its due date:

```json
{"expected_version":1,"status":"completed","due_date":null}
```

All PATCH requests require a positive `expected_version` from the last
GET/POST response and at least one changed field. Successful writes increment
the version. Stale versions return **409 Conflict** instead of silently
overwriting another edit. Task `due_date`, `description` and `project_id` may
be cleared with explicit JSON `null`; omitting a field preserves it.
Project slugs remain stable when renamed. Archiving a project preserves
its existing tasks but prevents new task assignments to it.

Responses have stable UUID IDs and JSON with metadata, timestamps and
version. These UUIDs are **not** the local SQLite integer IDs.

## Authorization and integrity

- All read queries join user/workspace ownership or explicit membership
  **inside PostgreSQL**. A personal workspace is owner-only.
- Viewer membership permits GET; editor permits GET/POST/PATCH; the workspace
  owner has full access. Nobody gains workspace rights merely from logging in.
- Every SQL mutation independently checks active user and workspace write
  permission. A caller cannot bypass this by using a different client.
- PostgreSQL enforces the composite (`workspace_id`, `project_id`) foreign key.
  A task cannot refer to a project in another workspace even if validation
  has a bug.
- Record UUIDs outside a caller's scope return **404**, indistinguishable
  from nonexistent UUIDs. Disabled users or revoked sessions get **401**.
- Invalid input gives **400**, stale versions or duplicate slugs give **409**,
  and backend failure gives **503**. Responses use `Cache-Control: no-store`.
- No hard delete, orphaning, background jobs or silent schema conversion.

## Test coverage

Locally:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --locked
python3 scripts/server-smoke.py ./target/debug/pos-server
```

GitHub Actions uses a disposable PostgreSQL 17 instance (`pos_ci`) and adds
`cargo test -p pos-server --test records`. It validates the actual HTTP
API, both domain validators, cross-user and cross-workspace access denial,
viewer/editor transitions, same-workspace project links, version conflicts,
nullable-field clearing, completing a task, archived projects and session
revocation. Local test runs without the disposable PostgreSQL URL compile but
skip this live integration test.

## Still to build

Pagination, filtering/search, full server-side task lifecycle, hard-delete
policy, auth provisioning/login APIs, TLS, rate limits, audited access,
comprehensive tenant hardening, device sync and offline conflict resolution.
The client CLI/TUI still uses local SQLite only.
