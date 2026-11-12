-- Phase 9b: authentication metadata only. No workspaces or business data.
-- Provisioning is internal/trusted only; no anonymous HTTP auth routes.
-- UUID keys are server-generated. Bearer secrets are stored ONLY as digests.
CREATE TABLE pos_users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    login_email TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    disabled_at TIMESTAMPTZ,
    CONSTRAINT pos_user_email_canonical CHECK (
        login_email = lower(login_email)
        AND char_length(login_email) BETWEEN 5 AND 254
        AND position('@' in login_email) > 1
    ),
    CONSTRAINT pos_password_hash_phc CHECK (password_hash LIKE '$argon2id$%')
);

CREATE TABLE pos_devices (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES pos_users(id) ON DELETE RESTRICT,
    label TEXT NOT NULL CHECK (char_length(trim(label)) BETWEEN 1 AND 120),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    revoked_at TIMESTAMPTZ,
    UNIQUE (id, user_id)
);

CREATE INDEX pos_devices_by_user ON pos_devices (user_id);

CREATE TABLE pos_sessions (
    token_hash BYTEA PRIMARY KEY CHECK (octet_length(token_hash) = 32),
    user_id UUID NOT NULL REFERENCES pos_users(id) ON DELETE RESTRICT,
    device_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    FOREIGN KEY (device_id, user_id)
        REFERENCES pos_devices (id, user_id) ON DELETE RESTRICT,
    CHECK (expires_at > created_at)
);

CREATE INDEX pos_sessions_by_user ON pos_sessions (user_id);
CREATE INDEX pos_sessions_by_device ON pos_sessions (device_id);
