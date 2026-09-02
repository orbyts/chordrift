-- Optional presentation metadata last verified by the configured product
-- identity provider. These fields are not authorization claims and never
-- replace the stable issuer/subject binding.

ALTER TABLE product_external_identities
    ADD COLUMN display_name TEXT
        CHECK (display_name IS NULL OR (btrim(display_name) <> '' AND length(display_name) <= 200)),
    ADD COLUMN avatar_url TEXT
        CHECK (avatar_url IS NULL OR (btrim(avatar_url) <> '' AND length(avatar_url) <= 2048)),
    ADD COLUMN profile_verified_at TIMESTAMPTZ;

COMMENT ON COLUMN product_external_identities.display_name IS
    'Optional presentation name last verified through OIDC UserInfo; never used for authorization.';
COMMENT ON COLUMN product_external_identities.avatar_url IS
    'Optional sanitized HTTPS profile-image URL last verified through OIDC UserInfo; never a credential.';
COMMENT ON COLUMN product_external_identities.profile_verified_at IS
    'Time at which presentation metadata was last refreshed from the configured identity provider.';
