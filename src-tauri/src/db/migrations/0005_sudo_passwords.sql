-- Per-profile sudo password, typed once at a `[sudo] password for` prompt
-- in a terminal and answered automatically after that. A missing row means
-- "remember (the default), nothing saved yet". `remember = 0` means the
-- user chose to be asked every time; `secret_id` is then always NULL.
CREATE TABLE sudo_passwords (
    ssh_profile_id  TEXT PRIMARY KEY REFERENCES ssh_profiles(id) ON DELETE CASCADE,
    secret_id       TEXT NULL REFERENCES secrets(id) ON DELETE SET NULL,
    remember        INTEGER NOT NULL DEFAULT 1,
    updated_at      TEXT NOT NULL
);
