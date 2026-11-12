-- Phase 9d. Server records are separate from local SQLite records.
-- Authorization is enforced on each repository operation. FK prevents
-- linking tasks to projects in another workspace even if application code fails.
-- Destructive deletes are intentionally not part of this initial API.

CREATE TABLE pos_projects (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID NOT NULL REFERENCES pos_workspaces(id) ON DELETE RESTRICT,
    name TEXT NOT NULL CHECK (octet_length(name) BETWEEN 1 AND 200),
    slug TEXT NOT NULL CHECK (
        char_length(slug) BETWEEN 1 AND 80
        AND slug ~ '^[a-z0-9]+(-[a-z0-9]+)*$'
    ),
    archived BOOLEAN NOT NULL DEFAULT FALSE,
    version BIGINT NOT NULL DEFAULT 1 CHECK (version > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (workspace_id, id),
    UNIQUE (workspace_id, slug)
);
CREATE INDEX pos_projects_by_workspace
    ON pos_projects (workspace_id, created_at, id);

CREATE TABLE pos_tasks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID NOT NULL REFERENCES pos_workspaces(id) ON DELETE RESTRICT,
    project_id UUID,
    title TEXT NOT NULL CHECK (octet_length(btrim(title)) BETWEEN 1 AND 500),
    description TEXT CHECK (description IS NULL OR octet_length(description) <= 20000),
    priority TEXT NOT NULL DEFAULT 'normal'
        CHECK (priority IN ('low', 'normal', 'high', 'urgent')),
    status TEXT NOT NULL DEFAULT 'todo'
        CHECK (status IN ('todo', 'in-progress', 'blocked', 'completed')),
    due_date DATE,
    version BIGINT NOT NULL DEFAULT 1 CHECK (version > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (workspace_id, project_id)
        REFERENCES pos_projects (workspace_id, id) ON DELETE RESTRICT
);
CREATE INDEX pos_tasks_by_workspace
    ON pos_tasks (workspace_id, created_at, id);
CREATE INDEX pos_tasks_by_project
    ON pos_tasks (workspace_id, project_id)
    WHERE project_id IS NOT NULL;
