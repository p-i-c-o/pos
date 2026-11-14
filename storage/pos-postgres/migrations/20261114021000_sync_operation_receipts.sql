-- Client mutation receipts: one applied operation per authenticated device.
CREATE TABLE pos_sync_operation_receipts (
    workspace_id UUID NOT NULL REFERENCES pos_workspaces(id) ON DELETE RESTRICT,
    device_id UUID NOT NULL REFERENCES pos_devices(id) ON DELETE RESTRICT,
    operation_id UUID NOT NULL,
    request JSONB NOT NULL,
    response JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (workspace_id, device_id, operation_id)
);
CREATE INDEX pos_sync_operation_receipts_device ON pos_sync_operation_receipts(device_id, created_at);
