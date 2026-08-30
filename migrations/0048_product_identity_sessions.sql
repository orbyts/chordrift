-- Product identity and revocable Chordrift sessions. External identity
-- verification remains provider-pluggable; this schema stores only stable
-- identity claims and SHA-256 digests of high-entropy Chordrift bearer tokens.

CREATE TABLE product_subjects (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    status TEXT NOT NULL DEFAULT 'active'
        CHECK (status IN ('active', 'suspended', 'closed')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE product_external_identities (
    issuer TEXT NOT NULL CHECK (btrim(issuer) <> ''),
    external_subject TEXT NOT NULL CHECK (btrim(external_subject) <> ''),
    product_subject_id UUID NOT NULL
        REFERENCES product_subjects (id) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'active'
        CHECK (status IN ('active', 'revoked')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (issuer, external_subject),
    UNIQUE (product_subject_id, issuer, external_subject)
);

CREATE TABLE chordrift_account_memberships (
    chordrift_account_id UUID NOT NULL
        REFERENCES chordrift_accounts (id) ON DELETE CASCADE,
    product_subject_id UUID NOT NULL
        REFERENCES product_subjects (id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('owner', 'member')),
    status TEXT NOT NULL DEFAULT 'active'
        CHECK (status IN ('active', 'revoked')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (chordrift_account_id, product_subject_id)
);

CREATE UNIQUE INDEX chordrift_account_memberships_active_owner_uq
    ON chordrift_account_memberships (chordrift_account_id)
    WHERE role = 'owner' AND status = 'active';

CREATE TABLE product_sessions (
    id UUID PRIMARY KEY,
    product_subject_id UUID NOT NULL,
    chordrift_account_id UUID NOT NULL,
    token_sha256 BYTEA NOT NULL UNIQUE CHECK (octet_length(token_sha256) = 32),
    created_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    CHECK (expires_at > created_at),
    CHECK (revoked_at IS NULL OR revoked_at >= created_at),
    FOREIGN KEY (chordrift_account_id, product_subject_id)
        REFERENCES chordrift_account_memberships
            (chordrift_account_id, product_subject_id) ON DELETE CASCADE
);

CREATE INDEX product_sessions_active_lookup_idx
    ON product_sessions (token_sha256)
    WHERE revoked_at IS NULL;
CREATE INDEX product_sessions_subject_idx
    ON product_sessions (product_subject_id, created_at DESC);
CREATE INDEX product_sessions_account_idx
    ON product_sessions (chordrift_account_id, created_at DESC);

COMMENT ON TABLE product_external_identities IS
    'Verified external issuer/subject bindings. No external access or refresh credential is stored here.';
COMMENT ON TABLE chordrift_account_memberships IS
    'Product authorization boundary between a subject and a Chordrift account.';
COMMENT ON COLUMN product_sessions.token_sha256 IS
    'One-way digest of a randomly generated Chordrift bearer token; plaintext is returned once and never persisted.';
