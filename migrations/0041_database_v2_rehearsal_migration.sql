-- Explicit database-v2 data migration support. This migration only adds
-- schema and helper functions; data movement requires an exact-confirmed
-- `chordrift db v2 migration apply` invocation.

ALTER TABLE listening_evidence_source_files
    ALTER COLUMN content_sha256 DROP NOT NULL,
    ADD COLUMN hash_status TEXT NOT NULL DEFAULT 'verified'
        CHECK (hash_status IN ('verified', 'archive_manifest_only')),
    ADD CONSTRAINT listening_evidence_source_file_hash_check CHECK (
        (hash_status = 'verified' AND content_sha256 IS NOT NULL)
        OR (hash_status = 'archive_manifest_only' AND content_sha256 IS NULL)
    );

COMMENT ON COLUMN listening_evidence_source_files.hash_status IS
    'Whether the member hash was verified directly or only the containing immutable archive manifest was available during legacy migration.';

ALTER TABLE external_playlist_cleanup_batches
    ADD COLUMN provider_checkpoint_id UUID
        REFERENCES provider_inventory_checkpoints (id) ON DELETE RESTRICT;

ALTER TABLE reevaluation_events
    ADD COLUMN provider_checkpoint_id UUID
        REFERENCES provider_inventory_checkpoints (id) ON DELETE RESTRICT;

CREATE TABLE database_v2_migration_runs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider_account_id UUID NOT NULL
        REFERENCES provider_accounts (id) ON DELETE CASCADE,
    plan_sha256 TEXT NOT NULL CHECK (plan_sha256 ~ '^[0-9a-f]{64}$'),
    migration_version TEXT NOT NULL CHECK (btrim(migration_version) <> ''),
    legacy_event_count BIGINT NOT NULL CHECK (legacy_event_count >= 0),
    normalized_event_count BIGINT NOT NULL CHECK (normalized_event_count >= 0),
    evidence_import_count INTEGER NOT NULL CHECK (evidence_import_count >= 0),
    checkpoint_count INTEGER NOT NULL CHECK (checkpoint_count >= 0),
    applied_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    verified_at TIMESTAMPTZ,
    verification JSONB NOT NULL DEFAULT '{}'::jsonb,
    UNIQUE (provider_account_id, plan_sha256)
);

COMMENT ON TABLE database_v2_migration_runs IS
    'Exact-confirmed database-v2 migration receipts; production execution requires separate operator approval.';

CREATE OR REPLACE FUNCTION materialize_provider_checkpoint_v2(
    p_provider_account_id UUID,
    p_snapshot_id UUID
)
RETURNS UUID
LANGUAGE plpgsql
AS $$
DECLARE
    v_checkpoint_id UUID;
BEGIN
    -- Reuse the content-addressed materializer. The caller restores the current
    -- pointer before committing, so intermediate pointer changes are invisible.
    PERFORM materialize_provider_current_state_v2(
        p_provider_account_id, p_snapshot_id
    );

    INSERT INTO provider_inventory_checkpoints
        (provider_account_id, provider, checkpoint_kind, label,
         state_sha256, source_snapshot_id, captured_at)
    SELECT inventory.provider_account_id, inventory.provider, 'named_baseline',
           'legacy durable audit baseline', inventory.state_sha256,
           p_snapshot_id, inventory.captured_at
      FROM provider_current_inventories inventory
     WHERE inventory.provider_account_id = p_provider_account_id
    ON CONFLICT (provider_account_id, checkpoint_kind, state_sha256)
    DO UPDATE SET released_at = NULL
    RETURNING id INTO v_checkpoint_id;

    INSERT INTO provider_inventory_checkpoint_playlists
        (checkpoint_id, provider_playlist_id, revision_id, name,
         description, public, collaborative, provider_revision)
    SELECT v_checkpoint_id, playlist.provider_playlist_id,
           playlist.revision_id, playlist.name, playlist.description,
           playlist.public, playlist.collaborative,
           playlist.provider_revision
      FROM provider_current_playlists playlist
     WHERE playlist.provider_account_id = p_provider_account_id
    ON CONFLICT (checkpoint_id, provider_playlist_id) DO NOTHING;

    INSERT INTO provider_inventory_checkpoint_saved_surfaces
        (checkpoint_id, saved_track_revision_id, saved_album_revision_id)
    SELECT v_checkpoint_id, inventory.saved_track_revision_id,
           inventory.saved_album_revision_id
      FROM provider_current_inventories inventory
     WHERE inventory.provider_account_id = p_provider_account_id
    ON CONFLICT (checkpoint_id) DO NOTHING;

    RETURN v_checkpoint_id;
END;
$$;

COMMENT ON FUNCTION materialize_provider_checkpoint_v2(UUID, UUID) IS
    'Builds or reuses a compact content-addressed named baseline for one legacy snapshot; callers must restore the current pointer in the same transaction.';
