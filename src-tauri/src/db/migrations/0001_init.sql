-- Core schema for Remotix local vault + connection inventory.
-- All timestamps are UTC ISO-8601 strings. All ids are UUID v4 strings.

CREATE TABLE vault_metadata (
    id                      TEXT PRIMARY KEY,
    format_version          INTEGER NOT NULL,
    salt                    BLOB NOT NULL,
    encrypted_check_value   BLOB NOT NULL,
    check_nonce             BLOB NOT NULL,
    created_at              TEXT NOT NULL
);

CREATE TABLE secrets (
    id              TEXT PRIMARY KEY,
    ciphertext      BLOB NOT NULL,
    nonce           BLOB NOT NULL,
    aad_version     INTEGER NOT NULL,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

CREATE TABLE terminal_groups (
    id          TEXT PRIMARY KEY,
    parent_id   TEXT NULL REFERENCES terminal_groups(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    sort_order  INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);
CREATE INDEX idx_terminal_groups_parent ON terminal_groups(parent_id);

CREATE TABLE ssh_profiles (
    id                      TEXT PRIMARY KEY,
    terminal_group_id       TEXT NULL REFERENCES terminal_groups(id) ON DELETE SET NULL,
    name                    TEXT NOT NULL,
    hostname                TEXT NOT NULL,
    port                    INTEGER NOT NULL DEFAULT 22,
    username                TEXT NOT NULL,
    authentication_kind     TEXT NOT NULL CHECK (authentication_kind IN ('password', 'private_key', 'private_key_passphrase')),
    secret_id               TEXT NULL REFERENCES secrets(id) ON DELETE SET NULL,
    passphrase_secret_id    TEXT NULL REFERENCES secrets(id) ON DELETE SET NULL,
    terminal_type           TEXT NOT NULL DEFAULT 'xterm-256color',
    sort_order              INTEGER NOT NULL DEFAULT 0,
    last_status             TEXT NULL,
    last_connected_at       TEXT NULL,
    created_at              TEXT NOT NULL,
    updated_at              TEXT NOT NULL
);
CREATE INDEX idx_ssh_profiles_group ON ssh_profiles(terminal_group_id);

CREATE TABLE known_hosts (
    id                      TEXT PRIMARY KEY,
    hostname                TEXT NOT NULL,
    port                    INTEGER NOT NULL,
    key_algorithm           TEXT NOT NULL,
    host_key                BLOB NOT NULL,
    sha256_fingerprint      TEXT NOT NULL,
    first_trusted_at        TEXT NOT NULL,
    last_seen_at            TEXT NOT NULL,
    UNIQUE(hostname, port)
);

CREATE TABLE port_forward_groups (
    id          TEXT PRIMARY KEY,
    parent_id   TEXT NULL REFERENCES port_forward_groups(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    sort_order  INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);
CREATE INDEX idx_port_forward_groups_parent ON port_forward_groups(parent_id);

CREATE TABLE port_forwards (
    id                      TEXT PRIMARY KEY,
    port_forward_group_id   TEXT NULL REFERENCES port_forward_groups(id) ON DELETE SET NULL,
    ssh_profile_id          TEXT NULL REFERENCES ssh_profiles(id) ON DELETE SET NULL,
    name                    TEXT NOT NULL,
    type                    TEXT NOT NULL CHECK (type IN ('local', 'remote', 'dynamic')),
    bind_address            TEXT NOT NULL DEFAULT '127.0.0.1',
    listen_port             INTEGER NOT NULL,
    target_hostname         TEXT NULL,
    target_port             INTEGER NULL,
    start_on_launch         INTEGER NOT NULL DEFAULT 0,
    sort_order              INTEGER NOT NULL DEFAULT 0,
    created_at              TEXT NOT NULL,
    updated_at              TEXT NOT NULL
);
CREATE INDEX idx_port_forwards_group ON port_forwards(port_forward_group_id);
CREATE INDEX idx_port_forwards_profile ON port_forwards(ssh_profile_id);

CREATE TABLE app_settings (
    key         TEXT PRIMARY KEY,
    value       TEXT NOT NULL
);
