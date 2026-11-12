-- Additive Task Engine v2 migration.
-- Preserve all existing IDs, project links, dates, and completed states.
ALTER TABLE tasks ADD COLUMN description TEXT CHECK(description IS NULL OR length(CAST(description AS BLOB)) <= 20000);
ALTER TABLE tasks ADD COLUMN priority TEXT NOT NULL DEFAULT 'normal'
    CHECK(priority IN ('low', 'normal', 'high', 'urgent'));
ALTER TABLE tasks ADD COLUMN status TEXT NOT NULL DEFAULT 'todo'
    CHECK(status IN ('todo', 'in-progress', 'blocked', 'completed'));
UPDATE tasks SET status = 'completed' WHERE completed = 1;
CREATE INDEX idx_tasks_status_priority ON tasks(status, priority, id);
-- Keep the legacy completed flag and the new workflow status consistent,
-- including for external writers. Repository operations update both together.
CREATE TRIGGER tasks_status_consistency_insert
BEFORE INSERT ON tasks
WHEN NEW.completed <> (NEW.status = 'completed')
BEGIN
    SELECT RAISE(ABORT, 'task status and completed flag must agree');
END;

CREATE TRIGGER tasks_status_consistency_update
BEFORE UPDATE ON tasks
WHEN NEW.completed <> (NEW.status = 'completed')
BEGIN
    SELECT RAISE(ABORT, 'task status and completed flag must agree');
END;
