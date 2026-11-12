-- Phase 9c: minimal workspace ownership and explicit memberships.
-- No Tasks/Projects tables or business-data endpoints are added here.
CREATE TABLE pos_workspaces (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    owner_user_id UUID NOT NULL REFERENCES pos_users(id) ON DELETE RESTRICT,
    kind TEXT NOT NULL CHECK (kind IN ('personal', 'shared')),
    name TEXT NOT NULL CHECK (char_length(btrim(name)) BETWEEN 1 AND 120),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- One active personal workspace per owner for this initial model.
CREATE UNIQUE INDEX pos_one_personal_workspace_per_user
    ON pos_workspaces (owner_user_id) WHERE kind = 'personal';

CREATE INDEX pos_workspaces_by_owner ON pos_workspaces (owner_user_id);

-- Owner is implicit through pos_workspaces.owner_user_id.
-- Grantable roles are deliberately less privileged than owner.
CREATE TABLE pos_workspace_members (
    workspace_id UUID NOT NULL REFERENCES pos_workspaces(id) ON DELETE RESTRICT,
    user_id UUID NOT NULL REFERENCES pos_users(id) ON DELETE RESTRICT,
    role TEXT NOT NULL CHECK (role IN ('viewer', 'editor')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (workspace_id, user_id)
);
CREATE INDEX pos_workspace_members_by_user ON pos_workspace_members (user_id);
