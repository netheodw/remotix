-- Per-profile terminal command history, used by the terminal's local
-- Up/Down navigation so history survives reconnects and app restarts.
-- Deliberately unencrypted (like hostnames and usernames): commands that
-- must not be stored can be prefixed with a space, and the whole log for a
-- profile can be cleared from the terminal tab.

CREATE TABLE command_history (
    id              TEXT PRIMARY KEY,
    ssh_profile_id  TEXT NOT NULL REFERENCES ssh_profiles(id) ON DELETE CASCADE,
    command         TEXT NOT NULL,
    executed_at     TEXT NOT NULL
);
CREATE INDEX idx_command_history_profile ON command_history(ssh_profile_id, executed_at);
