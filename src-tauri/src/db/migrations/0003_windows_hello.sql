-- Windows Hello unlock. The vault key is wrapped (AES-256-GCM) under a key
-- derived from a Windows Hello signature over `challenge`; the signing key
-- never leaves the TPM / Hello container, so this row alone cannot unlock
-- the vault. At most one row.
CREATE TABLE vault_hello (
    id              INTEGER PRIMARY KEY CHECK (id = 1),
    challenge       BLOB NOT NULL,
    wrapped_key     BLOB NOT NULL,
    nonce           BLOB NOT NULL,
    aad_version     INTEGER NOT NULL,
    created_at      TEXT NOT NULL
);

-- When the master password was last typed. Windows Hello unlock is refused
-- once this is more than six months old (or was never recorded), so the
-- password is asked for at least twice a year and is not forgotten.
ALTER TABLE vault_metadata ADD COLUMN password_verified_at TEXT NULL;
