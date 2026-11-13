-- Sync foundation v1. Applied only on explicit SqliteTaskRepository::open.
-- Keeps integer IDs and current Tasks/Projects contracts unchanged.
-- All existing records receive an immutable local identity, and their current
-- state is queued for initial reconciliation (nothing is transmitted).
ALTER TABLE projects ADD COLUMN sync_id TEXT
    CHECK (
        sync_id IS NULL OR (
            length(sync_id) = 36
            AND substr(sync_id, 9, 1) = '-'
            AND substr(sync_id, 14, 1) = '-'
            AND substr(sync_id, 19, 1) = '-'
            AND substr(sync_id, 24, 1) = '-'
            AND substr(sync_id, 15, 1) = '4'
            AND substr(sync_id, 20, 1) IN ('8', '9', 'a', 'b')
            AND length(replace(sync_id, '-', '')) = 32
            AND replace(sync_id, '-', '') NOT GLOB '*[^0-9a-f]*'
        )
    );
ALTER TABLE tasks ADD COLUMN sync_id TEXT
    CHECK (
        sync_id IS NULL OR (
            length(sync_id) = 36
            AND substr(sync_id, 9, 1) = '-'
            AND substr(sync_id, 14, 1) = '-'
            AND substr(sync_id, 19, 1) = '-'
            AND substr(sync_id, 24, 1) = '-'
            AND substr(sync_id, 15, 1) = '4'
            AND substr(sync_id, 20, 1) IN ('8', '9', 'a', 'b')
            AND length(replace(sync_id, '-', '')) = 32
            AND replace(sync_id, '-', '') NOT GLOB '*[^0-9a-f]*'
        )
    );

-- UUID v4, canonical lowercase representation, generated entirely by SQLite.
-- The literal expression is duplicated for rows written by external clients.
UPDATE projects SET sync_id =
    lower(hex(randomblob(4)) || '-' || hex(randomblob(2)) || '-4' ||
          substr(hex(randomblob(2)), 2) || '-' ||
          substr('89ab', 1 + (random() & 3), 1) ||
          substr(hex(randomblob(2)), 2) || '-' || hex(randomblob(6)));
UPDATE tasks SET sync_id =
    lower(hex(randomblob(4)) || '-' || hex(randomblob(2)) || '-4' ||
          substr(hex(randomblob(2)), 2) || '-' ||
          substr('89ab', 1 + (random() & 3), 1) ||
          substr(hex(randomblob(2)), 2) || '-' || hex(randomblob(6)));

CREATE UNIQUE INDEX pos_projects_sync_id ON projects(sync_id);
CREATE UNIQUE INDEX pos_tasks_sync_id ON tasks(sync_id);

-- Append-only sequence, never acknowledged in this release.
-- payload_json snapshots the write at mutation time. A delete is an explicit
-- tombstone with NULL payload. seq is scoped to this local database only.
CREATE TABLE pos_sync_outbox (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    format_version INTEGER NOT NULL DEFAULT 1 CHECK(format_version = 1),
    record_kind TEXT NOT NULL CHECK(record_kind IN ('task','project')),
    sync_id TEXT NOT NULL,
    operation TEXT NOT NULL CHECK(operation IN ('upsert','delete')),
    payload_json TEXT CHECK(payload_json IS NULL OR json_valid(payload_json)),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    CHECK ((operation = 'delete' AND payload_json IS NULL)
        OR (operation = 'upsert' AND payload_json IS NOT NULL))
);

CREATE INDEX pos_sync_outbox_record ON pos_sync_outbox(record_kind, sync_id, seq);

-- No consumer exists yet. Fail closed instead of letting an accidental SQL
-- statement erase or rewrite unacknowledged changes.
CREATE TRIGGER pos_sync_outbox_no_update
BEFORE UPDATE ON pos_sync_outbox
BEGIN
    SELECT RAISE(ABORT, 'sync journal is append-only');
END;
CREATE TRIGGER pos_sync_outbox_no_delete
BEFORE DELETE ON pos_sync_outbox
BEGIN
    SELECT RAISE(ABORT, 'sync journal is append-only');
END;

-- The initial import preserves preexisting local data without changing
-- user-visible IDs, and provides first-upload state even for old databases.
INSERT INTO pos_sync_outbox(record_kind, sync_id, operation, payload_json)
SELECT 'project', sync_id, 'upsert',
    json_object('sync_id', sync_id, 'name', name, 'slug', slug,
        'archived', json(CASE WHEN archived = 1 THEN 'true' ELSE 'false' END),
        'created_at', created_at, 'updated_at', updated_at)
FROM projects ORDER BY id;
INSERT INTO pos_sync_outbox(record_kind, sync_id, operation, payload_json)
SELECT 'task', t.sync_id, 'upsert',
    json_object('sync_id', t.sync_id, 'title', t.title,
        'description', t.description, 'priority', t.priority,
        'status', t.status, 'due_date', t.due_date,
        'completed', json(CASE WHEN t.completed = 1 THEN 'true' ELSE 'false' END),
        'project_sync_id', p.sync_id,
        'created_at', t.created_at, 'updated_at', t.updated_at)
FROM tasks AS t LEFT JOIN projects AS p ON p.id = t.project_id ORDER BY t.id;

-- IDs are immutable once assigned. Internal creation updates NULL -> UUID.
CREATE TRIGGER pos_project_sync_id_immutable
BEFORE UPDATE OF sync_id ON projects
WHEN OLD.sync_id IS NOT NULL AND NEW.sync_id IS NOT OLD.sync_id
BEGIN
    SELECT RAISE(ABORT, 'project sync identity is immutable');
END;
CREATE TRIGGER pos_task_sync_id_immutable
BEFORE UPDATE OF sync_id ON tasks
WHEN OLD.sync_id IS NOT NULL AND NEW.sync_id IS NOT OLD.sync_id
BEGIN
    SELECT RAISE(ABORT, 'task sync identity is immutable');
END;

-- NOTE: the initial INSERT trigger fills sync_id in-place first, then writes
-- exactly one create snapshot. The UPDATE trigger ignores the NULL -> UUID
-- transition so the internal update does not generate a duplicate event.
CREATE TRIGGER pos_project_sync_insert
AFTER INSERT ON projects
BEGIN
    UPDATE projects SET sync_id =
        lower(hex(randomblob(4)) || '-' || hex(randomblob(2)) || '-4' ||
              substr(hex(randomblob(2)), 2) || '-' ||
              substr('89ab', 1 + (random() & 3), 1) ||
              substr(hex(randomblob(2)), 2) || '-' || hex(randomblob(6)))
    WHERE id = NEW.id AND sync_id IS NULL;
    INSERT INTO pos_sync_outbox(record_kind, sync_id, operation, payload_json)
    SELECT 'project', sync_id, 'upsert',
        json_object('sync_id', sync_id, 'name', name, 'slug', slug,
            'archived', json(CASE WHEN archived = 1 THEN 'true' ELSE 'false' END),
            'created_at', created_at, 'updated_at', updated_at)
    FROM projects WHERE id = NEW.id;
END;

CREATE TRIGGER pos_task_sync_insert
AFTER INSERT ON tasks
BEGIN
    UPDATE tasks SET sync_id =
        lower(hex(randomblob(4)) || '-' || hex(randomblob(2)) || '-4' ||
              substr(hex(randomblob(2)), 2) || '-' ||
              substr('89ab', 1 + (random() & 3), 1) ||
              substr(hex(randomblob(2)), 2) || '-' || hex(randomblob(6)))
    WHERE id = NEW.id AND sync_id IS NULL;
    INSERT INTO pos_sync_outbox(record_kind, sync_id, operation, payload_json)
    SELECT 'task', t.sync_id, 'upsert',
        json_object('sync_id', t.sync_id, 'title', t.title,
            'description', t.description, 'priority', t.priority,
            'status', t.status, 'due_date', t.due_date,
            'completed', json(CASE WHEN t.completed = 1 THEN 'true' ELSE 'false' END),
            'project_sync_id', p.sync_id,
            'created_at', t.created_at, 'updated_at', t.updated_at)
    FROM tasks AS t LEFT JOIN projects AS p ON p.id = t.project_id
    WHERE t.id = NEW.id;
END;

CREATE TRIGGER pos_project_sync_update
AFTER UPDATE ON projects
WHEN OLD.sync_id IS NOT NULL AND NEW.sync_id IS OLD.sync_id
BEGIN
    INSERT INTO pos_sync_outbox(record_kind, sync_id, operation, payload_json)
    SELECT 'project', sync_id, 'upsert',
        json_object('sync_id', sync_id, 'name', name, 'slug', slug,
            'archived', json(CASE WHEN archived = 1 THEN 'true' ELSE 'false' END),
            'created_at', created_at, 'updated_at', updated_at)
    FROM projects WHERE id = NEW.id;
END;

CREATE TRIGGER pos_task_sync_update
AFTER UPDATE ON tasks
WHEN OLD.sync_id IS NOT NULL AND NEW.sync_id IS OLD.sync_id
BEGIN
    INSERT INTO pos_sync_outbox(record_kind, sync_id, operation, payload_json)
    SELECT 'task', t.sync_id, 'upsert',
        json_object('sync_id', t.sync_id, 'title', t.title,
            'description', t.description, 'priority', t.priority,
            'status', t.status, 'due_date', t.due_date,
            'completed', json(CASE WHEN t.completed = 1 THEN 'true' ELSE 'false' END),
            'project_sync_id', p.sync_id,
            'created_at', t.created_at, 'updated_at', t.updated_at)
    FROM tasks AS t LEFT JOIN projects AS p ON p.id = t.project_id
    WHERE t.id = NEW.id;
END;

CREATE TRIGGER pos_project_sync_delete
AFTER DELETE ON projects
BEGIN
    INSERT INTO pos_sync_outbox(record_kind, sync_id, operation, payload_json)
    VALUES ('project', OLD.sync_id, 'delete', NULL);
END;

CREATE TRIGGER pos_task_sync_delete
AFTER DELETE ON tasks
BEGIN
    INSERT INTO pos_sync_outbox(record_kind, sync_id, operation, payload_json)
    VALUES ('task', OLD.sync_id, 'delete', NULL);
END;
