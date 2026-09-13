-- Account service schema. Lives in its own database, separate from the relay,
-- so it can move to another host without touching relay data.

CREATE TABLE accounts (
    id                   UUID PRIMARY KEY,
    email                TEXT NOT NULL UNIQUE,
    pubkey               TEXT NOT NULL UNIQUE CHECK (pubkey ~ '^[0-9a-f]{64}$'),
    -- AES-256-GCM ciphertext of the 32-byte secret key under ACCOUNT_MASTER_KEY.
    encrypted_secret_key BYTEA NOT NULL CHECK (octet_length(encrypted_secret_key) BETWEEN 33 AND 128),
    key_nonce            BYTEA NOT NULL CHECK (length(key_nonce) = 12),
    created_at           TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- One outstanding code per email; a new request replaces the previous row.
CREATE TABLE login_codes (
    id          UUID PRIMARY KEY,
    email       TEXT NOT NULL UNIQUE,
    code_hash   BYTEA NOT NULL CHECK (length(code_hash) = 32),
    expires_at  TIMESTAMPTZ NOT NULL,
    attempts    INT NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    consumed_at TIMESTAMPTZ,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE sessions (
    id         UUID PRIMARY KEY,
    account_id UUID NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    token_hash BYTEA NOT NULL UNIQUE CHECK (length(token_hash) = 32),
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX sessions_account ON sessions (account_id);

-- account_id UNIQUE enforces "one community per account".
CREATE TABLE communities (
    id                 UUID PRIMARY KEY,
    account_id         UUID NOT NULL UNIQUE REFERENCES accounts (id) ON DELETE RESTRICT,
    name               TEXT NOT NULL,
    host               TEXT NOT NULL UNIQUE,
    relay_community_id UUID NOT NULL,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now()
);
