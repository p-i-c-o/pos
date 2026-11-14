-- PR #20: Durable, workspace-ordered change feed. All writers for one
-- workspace serialize on its clock row, so cursor order is commit order.
CREATE TABLE pos_sync_workspace_clocks (
    workspace_id UUID PRIMARY KEY REFERENCES pos_workspaces(id) ON DELETE RESTRICT,
    cursor BIGINT NOT NULL DEFAULT 0 CHECK (cursor >= 0)
);
CREATE TABLE pos_sync_changes (
    workspace_id UUID NOT NULL REFERENCES pos_workspaces(id) ON DELETE RESTRICT,
    cursor BIGINT NOT NULL CHECK (cursor > 0),
    record_kind TEXT NOT NULL CHECK (record_kind IN ('project', 'task')),
    record_id UUID NOT NULL,
    revision BIGINT NOT NULL CHECK (revision > 0),
    data JSONB NOT NULL,
    PRIMARY KEY (workspace_id, cursor)
);
CREATE INDEX pos_sync_changes_by_record ON pos_sync_changes(workspace_id, record_kind, record_id);

-- Backfill existing records before attaching triggers. The deterministic
-- initial cursor is reserved for the existing snapshot at migration time.
INSERT INTO pos_sync_changes (workspace_id, cursor, record_kind, record_id, revision, data)
SELECT workspace_id, ROW_NUMBER() OVER (PARTITION BY workspace_id ORDER BY kind, id),
       kind, id, version, data
FROM (
    SELECT workspace_id, 'project'::text AS kind, id, version, to_jsonb(p) AS data
      FROM pos_projects p
    UNION ALL
    SELECT workspace_id, 'task'::text AS kind, id, version, to_jsonb(t) AS data
      FROM pos_tasks t
) records;
INSERT INTO pos_sync_workspace_clocks (workspace_id, cursor)
SELECT workspace_id, MAX(cursor) FROM pos_sync_changes GROUP BY workspace_id;

CREATE FUNCTION pos_sync_capture_record() RETURNS trigger AS $$
DECLARE
    next_cursor BIGINT;
    kind TEXT;
BEGIN
    kind := CASE TG_TABLE_NAME WHEN 'pos_projects' THEN 'project' ELSE 'task' END;
    INSERT INTO pos_sync_workspace_clocks(workspace_id, cursor)
    VALUES (NEW.workspace_id, 0)
    ON CONFLICT (workspace_id) DO NOTHING;
    UPDATE pos_sync_workspace_clocks SET cursor = cursor + 1
    WHERE workspace_id = NEW.workspace_id RETURNING cursor INTO next_cursor;
    INSERT INTO pos_sync_changes(workspace_id, cursor, record_kind, record_id, revision, data)
    VALUES (NEW.workspace_id, next_cursor, kind, NEW.id, NEW.version, to_jsonb(NEW));
    RETURN NEW;
END
$$ LANGUAGE plpgsql;
CREATE TRIGGER pos_sync_project_changes
    AFTER INSERT OR UPDATE ON pos_projects
    FOR EACH ROW EXECUTE FUNCTION pos_sync_capture_record();
CREATE TRIGGER pos_sync_task_changes
    AFTER INSERT OR UPDATE ON pos_tasks
    FOR EACH ROW EXECUTE FUNCTION pos_sync_capture_record();
