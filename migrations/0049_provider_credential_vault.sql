-- Encrypted server-side provider credential vault. Ciphertext is bound to one
-- Chordrift account and provider account; encryption keys remain outside SQL.

CREATE TABLE provider_credential_vault (
    id UUID PRIMARY KEY,
    chordrift_account_id UUID NOT NULL,
    provider_account_id UUID NOT NULL,
    provider TEXT NOT NULL CHECK (btrim(provider) <> ''),
    credential_kind TEXT NOT NULL
        CHECK (credential_kind IN ('oauth_refresh')),
    generation INTEGER NOT NULL CHECK (generation > 0),
    algorithm TEXT NOT NULL
        CHECK (algorithm = 'xchacha20poly1305-v1'),
    key_id TEXT NOT NULL CHECK (btrim(key_id) <> ''),
    nonce BYTEA NOT NULL CHECK (octet_length(nonce) = 24),
    ciphertext BYTEA NOT NULL CHECK (octet_length(ciphertext) >= 16),
    created_by_subject_id UUID NOT NULL
        REFERENCES product_subjects (id) ON DELETE RESTRICT,
    created_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    revoked_by_subject_id UUID
        REFERENCES product_subjects (id) ON DELETE RESTRICT,
    revocation_reason TEXT,
    CHECK ((revoked_at IS NULL AND revoked_by_subject_id IS NULL
            AND revocation_reason IS NULL)
        OR (revoked_at IS NOT NULL AND revoked_by_subject_id IS NOT NULL
            AND revocation_reason IS NOT NULL
            AND btrim(revocation_reason) <> ''
            AND revoked_at >= created_at)),
    FOREIGN KEY (chordrift_account_id, provider_account_id)
        REFERENCES provider_accounts (chordrift_account_id, id)
        ON DELETE RESTRICT,
    FOREIGN KEY (provider_account_id, provider)
        REFERENCES provider_accounts (id, provider)
        ON DELETE RESTRICT,
    UNIQUE (provider_account_id, credential_kind, generation)
);

CREATE UNIQUE INDEX provider_credential_vault_active_uq
    ON provider_credential_vault (provider_account_id, credential_kind)
    WHERE revoked_at IS NULL;
CREATE INDEX provider_credential_vault_account_history_idx
    ON provider_credential_vault
       (chordrift_account_id, provider_account_id, credential_kind,
        generation DESC);

COMMENT ON TABLE provider_credential_vault IS
    'Authenticated provider credential ciphertext. Plaintext and root keys never enter PostgreSQL or client contracts.';
COMMENT ON COLUMN provider_credential_vault.nonce IS
    'Unique random XChaCha20-Poly1305 nonce; not secret.';
COMMENT ON COLUMN provider_credential_vault.key_id IS
    'External key-ring selector. Key material is not stored in this database.';
COMMENT ON COLUMN provider_credential_vault.ciphertext IS
    'AEAD ciphertext bound to credential ID, account, provider account, provider, kind, algorithm, and key ID.';
