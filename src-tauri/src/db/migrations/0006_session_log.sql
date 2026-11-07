-- Log of every terminal session (successful or not) and the commands run in
-- it, for looking back at what was executed. Separate from command_history,
-- which feeds Up/Down and can be cleared without touching this log.
-- The profile fields are copied in, so the log outlives a deleted profile.
-- Like command_history this is not encrypted; a command typed with a
-- leading space is not recorded.
CREATE TABLE terminal_sessions (
    id              TEXT PRIMARY KEY,
    ssh_profile_id  TEXT NULL REFERENCES ssh_profiles(id) ON DELETE SET NULL,
    profile_name    TEXT NOT NULL,
    hostname        TEXT NOT NULL,
    port            INTEGER NOT NULL,
    username        TEXT NOT NULL,
    started_at      TEXT NOT NULL,
    connected_at    TEXT NULL,
    ended_at        TEXT NULL,
    status          TEXT NOT NULL CHECK (status IN ('connecting', 'connected', 'failed', 'closed', 'interrupted')),
    end_reason      TEXT NULL
);
CREATE INDEX idx_terminal_sessions_started ON terminal_sessions(started_at);
CREATE INDEX idx_terminal_sessions_profile ON terminal_sessions(ssh_profile_id);

CREATE TABLE session_commands (
    id          TEXT PRIMARY KEY,
    session_id  TEXT NOT NULL REFERENCES terminal_sessions(id) ON DELETE CASCADE,
    command     TEXT NOT NULL,
    executed_at TEXT NOT NULL
);
CREATE INDEX idx_session_commands_session ON session_commands(session_id, executed_at);
