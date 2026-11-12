-- First server-side persistence migration. This is a database identity marker,
-- not yet a users/workspaces/tasks schema. Future data migrations must
-- define ownership and tenant isolation BEFORE adding shared records.
--
-- The initial instance UUID must remain stable across restarts.
CREATE TABLE pos_server_instance (
    singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton = TRUE),
    instance_id UUID NOT NULL DEFAULT gen_random_uuid(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

INSERT INTO pos_server_instance (singleton) VALUES (TRUE);
