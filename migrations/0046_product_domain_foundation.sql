-- Provider-neutral product ownership and intent foundation. This migration is
-- additive: existing provider, inventory, playlist, evidence, and publication
-- tables retain their v0.1.4 meaning and runtime paths.

CREATE TABLE chordrift_accounts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    display_name TEXT NOT NULL CHECK (btrim(display_name) <> ''),
    status TEXT NOT NULL DEFAULT 'active'
        CHECK (status IN ('active', 'suspended', 'closed')),
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb
        CHECK (jsonb_typeof(metadata) = 'object'),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE provider_accounts
    ADD COLUMN chordrift_account_id UUID;

INSERT INTO chordrift_accounts
    (id, display_name, metadata, created_at, updated_at)
SELECT
    md5('chordrift-account:' || provider || ':' || account_label)::uuid,
    COALESCE(NULLIF(btrim(display_name), ''), account_label),
    jsonb_build_object('compatibility_source', 'v0.1_provider_account'),
    created_at,
    updated_at
FROM provider_accounts
ON CONFLICT (id) DO NOTHING;

UPDATE provider_accounts
SET chordrift_account_id =
    md5('chordrift-account:' || provider || ':' || account_label)::uuid;

ALTER TABLE provider_accounts
    ALTER COLUMN chordrift_account_id SET NOT NULL,
    ADD CONSTRAINT provider_accounts_chordrift_account_fk
        FOREIGN KEY (chordrift_account_id)
        REFERENCES chordrift_accounts (id) ON DELETE RESTRICT,
    ADD CONSTRAINT provider_accounts_chordrift_id_uq
        UNIQUE (chordrift_account_id, id),
    ADD CONSTRAINT provider_accounts_id_provider_uq
        UNIQUE (id, provider);

CREATE FUNCTION ensure_provider_account_chordrift_owner()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF NEW.chordrift_account_id IS NULL THEN
        SELECT account.chordrift_account_id
        INTO NEW.chordrift_account_id
        FROM provider_accounts account
        WHERE account.provider = NEW.provider
          AND account.account_label = NEW.account_label;

        IF NEW.chordrift_account_id IS NULL THEN
            NEW.chordrift_account_id :=
                md5('chordrift-account:' || NEW.provider || ':' || NEW.account_label)::uuid;
            INSERT INTO chordrift_accounts (id, display_name, metadata)
            VALUES (
                NEW.chordrift_account_id,
                COALESCE(NULLIF(btrim(NEW.display_name), ''), NEW.account_label),
                jsonb_build_object('compatibility_source', 'v0.1_provider_account')
            )
            ON CONFLICT (id) DO NOTHING;
        END IF;
    END IF;
    RETURN NEW;
END
$$;

CREATE TRIGGER provider_accounts_ensure_chordrift_owner
BEFORE INSERT ON provider_accounts
FOR EACH ROW
EXECUTE FUNCTION ensure_provider_account_chordrift_owner();

COMMENT ON TABLE chordrift_accounts IS
    'Provider-neutral product ownership boundary. Credentials remain outside PostgreSQL.';
COMMENT ON COLUMN provider_accounts.chordrift_account_id IS
    'Owning Chordrift account. The insert trigger preserves v0.1.4 provider-account upserts by assigning a stable compatibility owner when omitted.';

CREATE TABLE provider_capability_observations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    chordrift_account_id UUID NOT NULL,
    provider_account_id UUID NOT NULL,
    provider_capabilities JSONB NOT NULL DEFAULT '{}'::jsonb
        CHECK (jsonb_typeof(provider_capabilities) = 'object'),
    evidence_capabilities JSONB NOT NULL DEFAULT '{}'::jsonb
        CHECK (jsonb_typeof(evidence_capabilities) = 'object'),
    input_fingerprint TEXT
        CHECK (input_fingerprint IS NULL OR input_fingerprint ~ '^[0-9a-f]{64}$'),
    observed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb
        CHECK (jsonb_typeof(metadata) = 'object'),
    FOREIGN KEY (chordrift_account_id, provider_account_id)
        REFERENCES provider_accounts (chordrift_account_id, id) ON DELETE CASCADE,
    UNIQUE (chordrift_account_id, provider_account_id, id)
);

CREATE INDEX provider_capability_observations_latest_idx
    ON provider_capability_observations
       (chordrift_account_id, provider_account_id, observed_at DESC, id DESC);
CREATE UNIQUE INDEX provider_capability_observations_input_uq
    ON provider_capability_observations (provider_account_id, input_fingerprint)
    WHERE input_fingerprint IS NOT NULL;

CREATE TABLE library_collections (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    chordrift_account_id UUID NOT NULL
        REFERENCES chordrift_accounts (id) ON DELETE CASCADE,
    stable_key TEXT NOT NULL CHECK (btrim(stable_key) <> ''),
    name TEXT NOT NULL CHECK (btrim(name) <> ''),
    description TEXT,
    status TEXT NOT NULL DEFAULT 'active'
        CHECK (status IN ('active', 'archived')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (chordrift_account_id, stable_key),
    UNIQUE (chordrift_account_id, id)
);

CREATE TABLE collection_relationships (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    chordrift_account_id UUID NOT NULL,
    parent_collection_id UUID NOT NULL,
    child_collection_id UUID NOT NULL,
    relationship TEXT NOT NULL DEFAULT 'navigation'
        CHECK (relationship IN ('navigation', 'related')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    retired_at TIMESTAMPTZ,
    CHECK (parent_collection_id <> child_collection_id),
    CHECK (retired_at IS NULL OR retired_at >= created_at),
    FOREIGN KEY (chordrift_account_id, parent_collection_id)
        REFERENCES library_collections (chordrift_account_id, id) ON DELETE CASCADE,
    FOREIGN KEY (chordrift_account_id, child_collection_id)
        REFERENCES library_collections (chordrift_account_id, id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX collection_relationships_active_uq
    ON collection_relationships
       (chordrift_account_id, parent_collection_id, child_collection_id, relationship)
    WHERE retired_at IS NULL;

CREATE TABLE collection_rule_revisions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    chordrift_account_id UUID NOT NULL,
    collection_id UUID NOT NULL,
    revision INTEGER NOT NULL CHECK (revision > 0),
    state TEXT NOT NULL DEFAULT 'proposed'
        CHECK (state IN ('proposed', 'approved', 'superseded')),
    rule_schema_version INTEGER NOT NULL DEFAULT 1 CHECK (rule_schema_version = 1),
    rule_document JSONB NOT NULL CHECK (jsonb_typeof(rule_document) = 'object'),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    approved_at TIMESTAMPTZ,
    CHECK ((state = 'approved' AND approved_at IS NOT NULL)
        OR (state <> 'approved' AND approved_at IS NULL)),
    FOREIGN KEY (chordrift_account_id, collection_id)
        REFERENCES library_collections (chordrift_account_id, id) ON DELETE CASCADE,
    UNIQUE (collection_id, revision),
    UNIQUE (chordrift_account_id, id)
);

CREATE TABLE track_collection_membership_revisions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    chordrift_account_id UUID NOT NULL,
    collection_id UUID NOT NULL,
    track_id UUID NOT NULL REFERENCES tracks (id) ON DELETE CASCADE,
    strength TEXT NOT NULL CHECK (strength IN (
        'hard_boundary', 'strong_preference', 'supporting_fact', 'proposed'
    )),
    provenance TEXT NOT NULL CHECK (provenance IN (
        'explicit_user', 'approved_rule', 'provider_fact', 'external_fact',
        'learned_affinity', 'review_proposal'
    )),
    confidence_basis_points INTEGER NOT NULL
        CHECK (confidence_basis_points BETWEEN 0 AND 10000),
    source_rule_revision_id UUID,
    reason JSONB NOT NULL DEFAULT '{}'::jsonb
        CHECK (jsonb_typeof(reason) = 'object'),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    superseded_at TIMESTAMPTZ,
    CHECK (superseded_at IS NULL OR superseded_at >= created_at),
    FOREIGN KEY (chordrift_account_id, collection_id)
        REFERENCES library_collections (chordrift_account_id, id) ON DELETE CASCADE,
    FOREIGN KEY (chordrift_account_id, source_rule_revision_id)
        REFERENCES collection_rule_revisions (chordrift_account_id, id)
        ON DELETE RESTRICT
);

CREATE UNIQUE INDEX track_collection_membership_revisions_active_uq
    ON track_collection_membership_revisions
       (chordrift_account_id, collection_id, track_id)
    WHERE superseded_at IS NULL;
CREATE INDEX track_collection_membership_revisions_track_idx
    ON track_collection_membership_revisions
       (chordrift_account_id, track_id, created_at DESC, id DESC);

CREATE TABLE playlist_surfaces (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    chordrift_account_id UUID NOT NULL
        REFERENCES chordrift_accounts (id) ON DELETE CASCADE,
    stable_key TEXT NOT NULL CHECK (btrim(stable_key) <> ''),
    name TEXT NOT NULL CHECK (btrim(name) <> ''),
    authority TEXT NOT NULL
        CHECK (authority IN ('provider', 'user', 'chordrift', 'collaborative')),
    purpose TEXT NOT NULL CHECK (purpose IN (
        'intake', 'collection_view', 'renewable_experience', 'utility', 'bookmark'
    )),
    refresh_policy TEXT NOT NULL CHECK (refresh_policy IN (
        'untouched', 'monitored', 'manual_spin', 'scheduled', 'provider_controlled'
    )),
    collection_id UUID,
    active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    FOREIGN KEY (chordrift_account_id, collection_id)
        REFERENCES library_collections (chordrift_account_id, id) ON DELETE RESTRICT,
    UNIQUE (chordrift_account_id, stable_key),
    UNIQUE (chordrift_account_id, id)
);

CREATE TABLE playlist_surface_provider_links (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    chordrift_account_id UUID NOT NULL,
    surface_id UUID NOT NULL,
    provider_account_id UUID NOT NULL,
    provider_namespace TEXT NOT NULL CHECK (provider_namespace ~ '^[a-z0-9_]+$'),
    provider_playlist_id UUID,
    provider_playlist_key TEXT
        CHECK (provider_playlist_key IS NULL OR btrim(provider_playlist_key) <> ''),
    state TEXT NOT NULL DEFAULT 'planned'
        CHECK (state IN ('planned', 'observed', 'active', 'retired')),
    first_linked_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_verified_at TIMESTAMPTZ,
    retired_at TIMESTAMPTZ,
    CHECK (provider_playlist_id IS NOT NULL OR provider_playlist_key IS NOT NULL
        OR state = 'planned'),
    CHECK ((state = 'retired' AND retired_at IS NOT NULL)
        OR (state <> 'retired' AND retired_at IS NULL)),
    FOREIGN KEY (chordrift_account_id, surface_id)
        REFERENCES playlist_surfaces (chordrift_account_id, id) ON DELETE CASCADE,
    FOREIGN KEY (chordrift_account_id, provider_account_id)
        REFERENCES provider_accounts (chordrift_account_id, id) ON DELETE CASCADE,
    FOREIGN KEY (provider_account_id, provider_namespace)
        REFERENCES provider_accounts (id, provider) ON DELETE CASCADE,
    FOREIGN KEY (provider_account_id, provider_playlist_id)
        REFERENCES provider_account_playlists (provider_account_id, provider_playlist_id)
        ON DELETE RESTRICT,
    UNIQUE (chordrift_account_id, id)
);

CREATE UNIQUE INDEX playlist_surface_provider_links_active_surface_uq
    ON playlist_surface_provider_links (surface_id, provider_account_id)
    WHERE state = 'active';
CREATE UNIQUE INDEX playlist_surface_provider_links_active_target_uq
    ON playlist_surface_provider_links
       (provider_account_id, provider_namespace, provider_playlist_key)
    WHERE state = 'active' AND provider_playlist_key IS NOT NULL;

CREATE TABLE playlist_track_directives (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    chordrift_account_id UUID NOT NULL,
    surface_id UUID NOT NULL,
    track_id UUID NOT NULL REFERENCES tracks (id) ON DELETE CASCADE,
    directive TEXT NOT NULL CHECK (directive IN ('include', 'exclude', 'pin')),
    pinned_position INTEGER CHECK (pinned_position IS NULL OR pinned_position >= 0),
    reason TEXT NOT NULL CHECK (btrim(reason) <> ''),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    superseded_at TIMESTAMPTZ,
    CHECK ((directive = 'pin' AND pinned_position IS NOT NULL)
        OR (directive <> 'pin' AND pinned_position IS NULL)),
    CHECK (superseded_at IS NULL OR superseded_at >= created_at),
    FOREIGN KEY (chordrift_account_id, surface_id)
        REFERENCES playlist_surfaces (chordrift_account_id, id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX playlist_track_directives_active_uq
    ON playlist_track_directives (chordrift_account_id, surface_id, track_id)
    WHERE superseded_at IS NULL;

CREATE TABLE playlist_recipes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    chordrift_account_id UUID NOT NULL
        REFERENCES chordrift_accounts (id) ON DELETE CASCADE,
    stable_key TEXT NOT NULL CHECK (btrim(stable_key) <> ''),
    name TEXT NOT NULL CHECK (btrim(name) <> ''),
    status TEXT NOT NULL DEFAULT 'active'
        CHECK (status IN ('active', 'archived')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (chordrift_account_id, stable_key),
    UNIQUE (chordrift_account_id, id)
);

CREATE TABLE playlist_recipe_revisions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    chordrift_account_id UUID NOT NULL,
    recipe_id UUID NOT NULL,
    revision INTEGER NOT NULL CHECK (revision > 0),
    schema_version INTEGER NOT NULL DEFAULT 1 CHECK (schema_version = 1),
    state TEXT NOT NULL DEFAULT 'draft'
        CHECK (state IN ('draft', 'approved', 'superseded')),
    recipe_document JSONB NOT NULL CHECK (jsonb_typeof(recipe_document) = 'object'),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    approved_at TIMESTAMPTZ,
    CHECK ((state = 'approved' AND approved_at IS NOT NULL)
        OR (state <> 'approved' AND approved_at IS NULL)),
    FOREIGN KEY (chordrift_account_id, recipe_id)
        REFERENCES playlist_recipes (chordrift_account_id, id) ON DELETE CASCADE,
    UNIQUE (recipe_id, revision),
    UNIQUE (chordrift_account_id, id)
);

CREATE TABLE playlist_recipe_dependencies (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    chordrift_account_id UUID NOT NULL,
    recipe_revision_id UUID NOT NULL,
    lane TEXT NOT NULL CHECK (lane IN (
        'discovery', 'emerging', 'familiar', 'high_rotation', 'dormant', 'recovery'
    )),
    dependency_kind TEXT NOT NULL
        CHECK (dependency_kind IN ('collection', 'evidence_capability', 'provider_capability')),
    collection_id UUID,
    capability TEXT CHECK (capability IS NULL OR capability ~ '^[a-z][a-z0-9_]*$'),
    allocation_weight INTEGER NOT NULL CHECK (allocation_weight BETWEEN 0 AND 65535),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((dependency_kind = 'collection' AND collection_id IS NOT NULL AND capability IS NULL)
        OR (dependency_kind <> 'collection' AND collection_id IS NULL AND capability IS NOT NULL)),
    FOREIGN KEY (chordrift_account_id, recipe_revision_id)
        REFERENCES playlist_recipe_revisions (chordrift_account_id, id) ON DELETE CASCADE,
    FOREIGN KEY (chordrift_account_id, collection_id)
        REFERENCES library_collections (chordrift_account_id, id) ON DELETE RESTRICT
);

CREATE INDEX playlist_recipe_dependencies_revision_idx
    ON playlist_recipe_dependencies (recipe_revision_id, lane, id);

ALTER TABLE provider_inventory_checkpoints
    ADD CONSTRAINT provider_inventory_checkpoints_account_id_uq
        UNIQUE (provider_account_id, id);

CREATE TABLE onboarding_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    chordrift_account_id UUID NOT NULL,
    provider_account_id UUID NOT NULL,
    provider_inventory_checkpoint_id UUID,
    capability_observation_id UUID,
    include_extended_history BOOLEAN NOT NULL DEFAULT FALSE,
    ignore_existing_intent BOOLEAN NOT NULL DEFAULT TRUE,
    status TEXT NOT NULL DEFAULT 'created' CHECK (status IN (
        'created', 'observing', 'audited', 'accepted', 'cancelled', 'failed'
    )),
    input_fingerprint TEXT
        CHECK (input_fingerprint IS NULL OR input_fingerprint ~ '^[0-9a-f]{64}$'),
    input_manifest JSONB NOT NULL DEFAULT '{}'::jsonb
        CHECK (jsonb_typeof(input_manifest) = 'object'),
    output_provenance JSONB NOT NULL DEFAULT '{}'::jsonb
        CHECK (jsonb_typeof(output_provenance) = 'object'),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at TIMESTAMPTZ,
    CHECK (completed_at IS NULL OR completed_at >= created_at),
    FOREIGN KEY (chordrift_account_id, provider_account_id)
        REFERENCES provider_accounts (chordrift_account_id, id) ON DELETE CASCADE,
    FOREIGN KEY (provider_account_id, provider_inventory_checkpoint_id)
        REFERENCES provider_inventory_checkpoints (provider_account_id, id)
        ON DELETE RESTRICT,
    FOREIGN KEY (chordrift_account_id, provider_account_id, capability_observation_id)
        REFERENCES provider_capability_observations
            (chordrift_account_id, provider_account_id, id) ON DELETE RESTRICT,
    UNIQUE (chordrift_account_id, id)
);

CREATE UNIQUE INDEX onboarding_sessions_reproducible_input_uq
    ON onboarding_sessions (chordrift_account_id, provider_account_id, input_fingerprint)
    WHERE input_fingerprint IS NOT NULL;
CREATE INDEX onboarding_sessions_account_created_idx
    ON onboarding_sessions (chordrift_account_id, created_at DESC, id DESC);

CREATE TABLE playlist_spins (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    chordrift_account_id UUID NOT NULL,
    recipe_revision_id UUID NOT NULL,
    onboarding_session_id UUID,
    input_fingerprint TEXT NOT NULL CHECK (input_fingerprint ~ '^[0-9a-f]{64}$'),
    seed NUMERIC(20, 0) NOT NULL
        CHECK (seed BETWEEN 0 AND 18446744073709551615),
    capability_snapshot JSONB NOT NULL DEFAULT '{}'::jsonb
        CHECK (jsonb_typeof(capability_snapshot) = 'object'),
    status TEXT NOT NULL DEFAULT 'preview'
        CHECK (status IN ('preview', 'approved', 'superseded')),
    generated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    approved_at TIMESTAMPTZ,
    CHECK ((status = 'approved' AND approved_at IS NOT NULL)
        OR (status <> 'approved' AND approved_at IS NULL)),
    FOREIGN KEY (chordrift_account_id, recipe_revision_id)
        REFERENCES playlist_recipe_revisions (chordrift_account_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (chordrift_account_id, onboarding_session_id)
        REFERENCES onboarding_sessions (chordrift_account_id, id) ON DELETE RESTRICT,
    UNIQUE (chordrift_account_id, input_fingerprint, seed),
    UNIQUE (chordrift_account_id, id)
);

CREATE TABLE playlist_spin_tracks (
    spin_id UUID NOT NULL REFERENCES playlist_spins (id) ON DELETE CASCADE,
    track_id UUID NOT NULL REFERENCES tracks (id) ON DELETE RESTRICT,
    position INTEGER NOT NULL CHECK (position >= 0),
    lane TEXT NOT NULL CHECK (lane IN (
        'discovery', 'emerging', 'familiar', 'high_rotation', 'dormant', 'recovery'
    )),
    selection_reason JSONB NOT NULL CHECK (jsonb_typeof(selection_reason) = 'object'),
    ordering_reason JSONB NOT NULL CHECK (jsonb_typeof(ordering_reason) = 'object'),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (spin_id, position),
    UNIQUE (spin_id, track_id)
);

ALTER TABLE sync_runs
    ADD CONSTRAINT sync_runs_provider_namespace_fk
        FOREIGN KEY (provider_account_id, provider)
        REFERENCES provider_accounts (id, provider) ON DELETE CASCADE,
    ADD CONSTRAINT sync_runs_provider_account_id_uq
        UNIQUE (provider_account_id, id);
ALTER TABLE sync_readiness_assessments
    ADD CONSTRAINT sync_readiness_assessments_account_id_uq
        UNIQUE (provider_account_id, id);
ALTER TABLE sync_apply_runs
    ADD CONSTRAINT sync_apply_runs_account_id_uq
        UNIQUE (provider_account_id, id);
ALTER TABLE managed_playlist_verifications
    ADD CONSTRAINT managed_playlist_verifications_account_id_uq
        UNIQUE (provider_account_id, id);

CREATE TABLE playlist_spin_publications (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    chordrift_account_id UUID NOT NULL,
    spin_id UUID NOT NULL,
    surface_id UUID NOT NULL,
    provider_account_id UUID NOT NULL,
    sync_run_id UUID NOT NULL,
    readiness_assessment_id UUID,
    apply_run_id UUID,
    verification_id UUID,
    status TEXT NOT NULL DEFAULT 'planned'
        CHECK (status IN ('planned', 'approved', 'applying', 'applied', 'verified', 'superseded')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    approved_at TIMESTAMPTZ,
    verified_at TIMESTAMPTZ,
    CHECK (status <> 'verified' OR (verification_id IS NOT NULL AND verified_at IS NOT NULL)),
    FOREIGN KEY (chordrift_account_id, spin_id)
        REFERENCES playlist_spins (chordrift_account_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (chordrift_account_id, surface_id)
        REFERENCES playlist_surfaces (chordrift_account_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (chordrift_account_id, provider_account_id)
        REFERENCES provider_accounts (chordrift_account_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (provider_account_id, sync_run_id)
        REFERENCES sync_runs (provider_account_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (provider_account_id, readiness_assessment_id)
        REFERENCES sync_readiness_assessments (provider_account_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (provider_account_id, apply_run_id)
        REFERENCES sync_apply_runs (provider_account_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (provider_account_id, verification_id)
        REFERENCES managed_playlist_verifications (provider_account_id, id) ON DELETE RESTRICT,
    UNIQUE (spin_id, surface_id, sync_run_id)
);

CREATE INDEX playlist_spin_publications_account_created_idx
    ON playlist_spin_publications (chordrift_account_id, created_at DESC, id DESC);

COMMENT ON TABLE provider_capability_observations IS
    'Immutable account-scoped provider and evidence capability snapshots; omitted capabilities are unavailable in the Rust domain.';
COMMENT ON TABLE library_collections IS
    'Provider-neutral overlapping library map. Existing playlist_concepts remain provider-account canonical-output lineage.';
COMMENT ON TABLE playlist_surfaces IS
    'Provider-neutral surface intent. Existing provider policies, bookmarks, routes, and playlists remain observation and publication records.';
COMMENT ON TABLE playlist_recipe_revisions IS
    'Immutable versioned recipe documents; queryable collection and capability sources are normalized in playlist_recipe_dependencies.';
COMMENT ON TABLE onboarding_sessions IS
    'Provider-read-only onboarding boundary. Runtime behavior begins in V020-06; this table alone authorizes no provider write.';
COMMENT ON TABLE playlist_spins IS
    'Deterministic provider-free generation identity. Selection and ordering behavior begins in later roadmap slices.';
COMMENT ON TABLE playlist_spin_publications IS
    'Link from one approved Spin and surface to the existing immutable plan/readiness/apply/verification audit chain.';
