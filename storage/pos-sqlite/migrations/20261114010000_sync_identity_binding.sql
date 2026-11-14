-- PR #19: one SQLite database, one explicitly verified server workspace.
-- No network I/O is performed by migrations. Existing Tasks/Projects and
-- their append-only journal are untouched.
CREATE TABLE pos_sync_database (
    slot INTEGER PRIMARY KEY CHECK(slot = 1),
    database_id TEXT NOT NULL UNIQUE CHECK (
        length(database_id) = 36 AND
        substr(database_id, 15, 1) = '4' AND
        substr(database_id, 20, 1) IN ('8','9','a','b') AND
        length(replace(database_id, '-', '')) = 32 AND
        replace(database_id, '-', '') NOT GLOB '*[^0-9a-f]*'
    ),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
INSERT INTO pos_sync_database(slot, database_id) VALUES (
    1,
    lower(hex(randomblob(4)) || '-' || hex(randomblob(2)) || '-4' ||
          substr(hex(randomblob(2)), 2) || '-' ||
          substr('89ab', 1 + (random() & 3), 1) ||
          substr(hex(randomblob(2)), 2) || '-' || hex(randomblob(6)))
);
CREATE TRIGGER pos_sync_database_no_update BEFORE UPDATE ON pos_sync_database
BEGIN SELECT RAISE(ABORT, 'database sync identity is immutable'); END;
CREATE TRIGGER pos_sync_database_no_delete BEFORE DELETE ON pos_sync_database
BEGIN SELECT RAISE(ABORT, 'database sync identity is immutable'); END;

-- Immutable single-workspace binding. An approved future migration would
-- need a dedicated safe reassignment procedure; no unbind exists today.
CREATE TABLE pos_sync_binding (
    slot INTEGER PRIMARY KEY CHECK(slot = 1),
    database_id TEXT NOT NULL REFERENCES pos_sync_database(database_id),
    server_origin TEXT NOT NULL CHECK(length(server_origin) BETWEEN 12 AND 255),
    user_id TEXT NOT NULL CHECK(length(user_id) = 36),
    device_id TEXT NOT NULL CHECK(length(device_id) = 36),
    workspace_id TEXT NOT NULL CHECK(length(workspace_id) = 36),
    canonical_path TEXT NOT NULL CHECK(length(canonical_path) > 0),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
CREATE TRIGGER pos_sync_binding_no_update BEFORE UPDATE ON pos_sync_binding
BEGIN SELECT RAISE(ABORT, 'workspace binding is immutable'); END;
CREATE TRIGGER pos_sync_binding_no_delete BEFORE DELETE ON pos_sync_binding
BEGIN SELECT RAISE(ABORT, 'workspace binding is immutable'); END;

-- Internal mapping foundation. Insert only after a separately validated
-- server record response. The local ID is NOT the server record UUID.
-- No rows are created by this migration and no sync worker consumes them.
CREATE TABLE pos_sync_record_map (
    binding_slot INTEGER NOT NULL DEFAULT 1
        REFERENCES pos_sync_binding(slot) ON DELETE RESTRICT
        CHECK(binding_slot = 1),
    record_kind TEXT NOT NULL CHECK(record_kind IN ('task','project')),
    local_sync_id TEXT NOT NULL CHECK(length(local_sync_id) = 36),
    server_record_id TEXT NOT NULL CHECK(length(server_record_id) = 36),
    server_version INTEGER NOT NULL CHECK(server_version >= 1),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY(record_kind, local_sync_id),
    UNIQUE(record_kind, server_record_id)
);
CREATE TRIGGER pos_sync_record_map_no_delete BEFORE DELETE ON pos_sync_record_map
BEGIN SELECT RAISE(ABORT, 'record mappings cannot be deleted'); END;
CREATE TRIGGER pos_sync_record_map_identity_immutable BEFORE UPDATE ON pos_sync_record_map
WHEN NEW.binding_slot IS NOT OLD.binding_slot
  OR NEW.record_kind IS NOT OLD.record_kind
  OR NEW.local_sync_id IS NOT OLD.local_sync_id
  OR NEW.server_record_id IS NOT OLD.server_record_id
  OR NEW.server_version <= OLD.server_version
BEGIN SELECT RAISE(ABORT, 'record mapping identity immutable; version must increase'); END;
