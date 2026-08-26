-- Database-v2 storage foundation. This migration is additive and does not
-- delete or rewrite legacy provider snapshots or listening events.

CREATE TABLE provider_playlist_revisions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider_playlist_id UUID NOT NULL
        REFERENCES provider_playlists (id) ON DELETE CASCADE,
    provider_revision TEXT,
    content_sha256 TEXT NOT NULL CHECK (content_sha256 ~ '^[0-9a-f]{64}$'),
    item_count INTEGER NOT NULL CHECK (item_count >= 0),
    first_observed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_observed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (provider_playlist_id, content_sha256)
);

CREATE INDEX provider_playlist_revisions_provider_revision_idx
    ON provider_playlist_revisions (provider_playlist_id, provider_revision)
    WHERE provider_revision IS NOT NULL;

CREATE TABLE provider_playlist_revision_tracks (
    revision_id UUID NOT NULL
        REFERENCES provider_playlist_revisions (id) ON DELETE CASCADE,
    provider_track_id UUID NOT NULL
        REFERENCES provider_tracks (id) ON DELETE RESTRICT,
    position INTEGER NOT NULL CHECK (position >= 0),
    added_at TIMESTAMPTZ,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    PRIMARY KEY (revision_id, position)
);

CREATE INDEX provider_playlist_revision_tracks_track_idx
    ON provider_playlist_revision_tracks (provider_track_id);

CREATE TABLE provider_saved_track_revisions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider_account_id UUID NOT NULL
        REFERENCES provider_accounts (id) ON DELETE CASCADE,
    content_sha256 TEXT NOT NULL CHECK (content_sha256 ~ '^[0-9a-f]{64}$'),
    item_count INTEGER NOT NULL CHECK (item_count >= 0),
    first_observed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_observed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (provider_account_id, content_sha256)
);

CREATE TABLE provider_saved_track_revision_tracks (
    revision_id UUID NOT NULL
        REFERENCES provider_saved_track_revisions (id) ON DELETE CASCADE,
    provider_track_id UUID NOT NULL
        REFERENCES provider_tracks (id) ON DELETE RESTRICT,
    position INTEGER NOT NULL CHECK (position >= 0),
    saved_at TIMESTAMPTZ,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    PRIMARY KEY (revision_id, position)
);

CREATE INDEX provider_saved_track_revision_tracks_track_idx
    ON provider_saved_track_revision_tracks (provider_track_id);

CREATE TABLE provider_saved_album_revisions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider_account_id UUID NOT NULL
        REFERENCES provider_accounts (id) ON DELETE CASCADE,
    content_sha256 TEXT NOT NULL CHECK (content_sha256 ~ '^[0-9a-f]{64}$'),
    album_count INTEGER NOT NULL CHECK (album_count >= 0),
    track_count INTEGER NOT NULL CHECK (track_count >= 0),
    first_observed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_observed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (provider_account_id, content_sha256)
);

CREATE TABLE provider_saved_album_revision_albums (
    revision_id UUID NOT NULL
        REFERENCES provider_saved_album_revisions (id) ON DELETE CASCADE,
    provider_album_id UUID NOT NULL
        REFERENCES provider_albums (id) ON DELETE RESTRICT,
    position INTEGER NOT NULL CHECK (position >= 0),
    saved_at TIMESTAMPTZ,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    PRIMARY KEY (revision_id, position)
);

CREATE INDEX provider_saved_album_revision_albums_album_idx
    ON provider_saved_album_revision_albums (provider_album_id);

CREATE TABLE provider_saved_album_revision_tracks (
    revision_id UUID NOT NULL
        REFERENCES provider_saved_album_revisions (id) ON DELETE CASCADE,
    provider_album_id UUID NOT NULL
        REFERENCES provider_albums (id) ON DELETE RESTRICT,
    provider_track_id UUID NOT NULL
        REFERENCES provider_tracks (id) ON DELETE RESTRICT,
    position INTEGER NOT NULL CHECK (position >= 0),
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    PRIMARY KEY (revision_id, provider_album_id, position)
);

CREATE INDEX provider_saved_album_revision_tracks_track_idx
    ON provider_saved_album_revision_tracks (provider_track_id);

CREATE TABLE provider_current_inventories (
    provider_account_id UUID PRIMARY KEY
        REFERENCES provider_accounts (id) ON DELETE CASCADE,
    provider TEXT NOT NULL CHECK (btrim(provider) <> ''),
    source_snapshot_id UUID
        REFERENCES provider_library_snapshots (id) ON DELETE SET NULL,
    saved_track_revision_id UUID NOT NULL
        REFERENCES provider_saved_track_revisions (id) ON DELETE RESTRICT,
    saved_album_revision_id UUID NOT NULL
        REFERENCES provider_saved_album_revisions (id) ON DELETE RESTRICT,
    state_sha256 TEXT NOT NULL CHECK (state_sha256 ~ '^[0-9a-f]{64}$'),
    captured_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE provider_current_playlists (
    provider_account_id UUID NOT NULL
        REFERENCES provider_current_inventories (provider_account_id)
        ON DELETE CASCADE,
    provider_playlist_id UUID NOT NULL
        REFERENCES provider_playlists (id) ON DELETE RESTRICT,
    revision_id UUID NOT NULL
        REFERENCES provider_playlist_revisions (id) ON DELETE RESTRICT,
    name TEXT NOT NULL CHECK (btrim(name) <> ''),
    description TEXT,
    public BOOLEAN,
    collaborative BOOLEAN NOT NULL DEFAULT FALSE,
    provider_revision TEXT,
    reported_item_count INTEGER NOT NULL CHECK (reported_item_count >= 0),
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    PRIMARY KEY (provider_account_id, provider_playlist_id)
);

CREATE INDEX provider_current_playlists_revision_idx
    ON provider_current_playlists (revision_id);

CREATE TABLE provider_inventory_checkpoints (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider_account_id UUID NOT NULL
        REFERENCES provider_accounts (id) ON DELETE CASCADE,
    provider TEXT NOT NULL CHECK (btrim(provider) <> ''),
    checkpoint_kind TEXT NOT NULL
        CHECK (checkpoint_kind IN ('pre_apply', 'named_baseline')),
    label TEXT,
    state_sha256 TEXT NOT NULL CHECK (state_sha256 ~ '^[0-9a-f]{64}$'),
    source_snapshot_id UUID
        REFERENCES provider_library_snapshots (id) ON DELETE SET NULL,
    captured_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ,
    released_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (checkpoint_kind = 'pre_apply' OR label IS NOT NULL),
    CHECK (expires_at IS NULL OR expires_at > created_at),
    UNIQUE (provider_account_id, checkpoint_kind, state_sha256)
);

CREATE INDEX provider_inventory_checkpoints_retention_idx
    ON provider_inventory_checkpoints (provider_account_id, expires_at)
    WHERE released_at IS NULL;

CREATE TABLE provider_inventory_checkpoint_playlists (
    checkpoint_id UUID NOT NULL
        REFERENCES provider_inventory_checkpoints (id) ON DELETE CASCADE,
    provider_playlist_id UUID NOT NULL
        REFERENCES provider_playlists (id) ON DELETE RESTRICT,
    revision_id UUID NOT NULL
        REFERENCES provider_playlist_revisions (id) ON DELETE RESTRICT,
    name TEXT NOT NULL CHECK (btrim(name) <> ''),
    description TEXT,
    public BOOLEAN,
    collaborative BOOLEAN NOT NULL DEFAULT FALSE,
    provider_revision TEXT,
    PRIMARY KEY (checkpoint_id, provider_playlist_id)
);

CREATE TABLE provider_inventory_checkpoint_saved_surfaces (
    checkpoint_id UUID PRIMARY KEY
        REFERENCES provider_inventory_checkpoints (id) ON DELETE CASCADE,
    saved_track_revision_id UUID NOT NULL
        REFERENCES provider_saved_track_revisions (id) ON DELETE RESTRICT,
    saved_album_revision_id UUID NOT NULL
        REFERENCES provider_saved_album_revisions (id) ON DELETE RESTRICT
);

ALTER TABLE sync_runs
    ADD COLUMN provider_checkpoint_id UUID
        REFERENCES provider_inventory_checkpoints (id) ON DELETE RESTRICT;

ALTER TABLE managed_playlist_verifications
    ADD COLUMN provider_checkpoint_id UUID
        REFERENCES provider_inventory_checkpoints (id) ON DELETE RESTRICT;

CREATE TABLE listening_evidence_imports (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider_account_id UUID NOT NULL
        REFERENCES provider_accounts (id) ON DELETE CASCADE,
    provider TEXT NOT NULL CHECK (btrim(provider) <> ''),
    archive_kind TEXT NOT NULL CHECK (btrim(archive_kind) <> ''),
    archive_sha256 TEXT NOT NULL CHECK (archive_sha256 ~ '^[0-9a-f]{64}$'),
    parser_version TEXT NOT NULL CHECK (btrim(parser_version) <> ''),
    source_filename TEXT NOT NULL CHECK (btrim(source_filename) <> ''),
    source_file_count INTEGER NOT NULL CHECK (source_file_count >= 0),
    event_count BIGINT NOT NULL CHECK (event_count >= 0),
    first_event_at TIMESTAMPTZ,
    last_event_at TIMESTAMPTZ,
    manifest JSONB NOT NULL DEFAULT '{}'::jsonb,
    imported_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (provider_account_id, provider, archive_sha256)
);

CREATE TABLE listening_evidence_source_files (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    import_id UUID NOT NULL
        REFERENCES listening_evidence_imports (id) ON DELETE CASCADE,
    source_path TEXT NOT NULL CHECK (btrim(source_path) <> ''),
    content_sha256 TEXT NOT NULL CHECK (content_sha256 ~ '^[0-9a-f]{64}$'),
    event_count BIGINT NOT NULL CHECK (event_count >= 0),
    UNIQUE (import_id, source_path),
    UNIQUE (import_id, content_sha256)
);

CREATE TABLE historical_provider_track_identities (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider TEXT NOT NULL CHECK (btrim(provider) <> ''),
    provider_track_id TEXT NOT NULL CHECK (btrim(provider_track_id) <> ''),
    canonical_track_id UUID REFERENCES tracks (id) ON DELETE SET NULL,
    track_name TEXT,
    artist_name TEXT,
    album_name TEXT,
    first_observed_at TIMESTAMPTZ NOT NULL,
    last_observed_at TIMESTAMPTZ NOT NULL,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    CHECK (last_observed_at >= first_observed_at),
    UNIQUE (provider, provider_track_id)
);

CREATE INDEX historical_provider_track_identities_canonical_idx
    ON historical_provider_track_identities (canonical_track_id)
    WHERE canonical_track_id IS NOT NULL;

CREATE TABLE normalized_listening_events (
    id UUID PRIMARY KEY,
    provider_account_id UUID NOT NULL
        REFERENCES provider_accounts (id) ON DELETE CASCADE,
    historical_identity_id UUID NOT NULL
        REFERENCES historical_provider_track_identities (id) ON DELETE RESTRICT,
    source_import_id UUID
        REFERENCES listening_evidence_imports (id) ON DELETE RESTRICT,
    source_file_id UUID
        REFERENCES listening_evidence_source_files (id) ON DELETE RESTRICT,
    source_kind TEXT NOT NULL CHECK (source_kind IN ('archive', 'recent_api')),
    source_event_id TEXT,
    source_occurrence INTEGER NOT NULL DEFAULT 0 CHECK (source_occurrence >= 0),
    played_at TIMESTAMPTZ NOT NULL,
    ms_played INTEGER CHECK (ms_played IS NULL OR ms_played >= 0),
    skipped BOOLEAN,
    completed BOOLEAN,
    completion_reason TEXT,
    context_uri TEXT,
    context_type TEXT,
    superseded_at TIMESTAMPTZ,
    provider_extensions JSONB NOT NULL DEFAULT '{}'::jsonb,
    CHECK (source_kind <> 'archive' OR source_import_id IS NOT NULL),
    CHECK (source_kind <> 'recent_api' OR source_event_id IS NOT NULL)
);

CREATE UNIQUE INDEX normalized_listening_events_source_event_uq
    ON normalized_listening_events (provider_account_id, source_event_id)
    WHERE source_event_id IS NOT NULL;

CREATE UNIQUE INDEX normalized_listening_events_core_event_uq
    ON normalized_listening_events
       (provider_account_id, historical_identity_id, played_at,
        ms_played, source_occurrence)
    WHERE ms_played IS NOT NULL;

CREATE INDEX normalized_listening_events_account_time_idx
    ON normalized_listening_events (provider_account_id, played_at DESC)
    WHERE superseded_at IS NULL;

CREATE INDEX normalized_listening_events_identity_time_idx
    ON normalized_listening_events (historical_identity_id, played_at DESC)
    WHERE superseded_at IS NULL;

CREATE OR REPLACE FUNCTION materialize_provider_current_state_v2(
    p_provider_account_id UUID,
    p_snapshot_id UUID
)
RETURNS VOID
LANGUAGE plpgsql
AS $$
DECLARE
    v_provider TEXT;
    v_captured_at TIMESTAMPTZ;
    v_saved_track_hash TEXT;
    v_saved_track_revision_id UUID;
    v_saved_album_hash TEXT;
    v_saved_album_revision_id UUID;
    v_state_hash TEXT;
BEGIN
    SELECT snapshot.provider, snapshot.captured_at
      INTO v_provider, v_captured_at
      FROM provider_library_snapshots snapshot
     WHERE snapshot.id = p_snapshot_id
       AND snapshot.provider_account_id = p_provider_account_id;
    IF v_provider IS NULL THEN
        RAISE EXCEPTION 'snapshot does not belong to provider account';
    END IF;

    WITH playlist_hashes AS (
        SELECT observed.provider_playlist_id,
               encode(sha256(convert_to(COALESCE(string_agg(
                   membership.position::text || ':' || track.provider || ':' ||
                   track.provider_track_id, E'\n'
                   ORDER BY membership.position, track.provider_track_id
               ), ''), 'UTF8')), 'hex') AS content_sha256,
               count(membership.provider_track_id)::integer AS item_count,
               observed.provider_snapshot_id
          FROM provider_playlist_snapshots observed
          LEFT JOIN provider_playlist_tracks membership
            ON membership.snapshot_id = observed.snapshot_id
           AND membership.provider_playlist_id = observed.provider_playlist_id
          LEFT JOIN provider_tracks track ON track.id = membership.provider_track_id
         WHERE observed.snapshot_id = p_snapshot_id
         GROUP BY observed.provider_playlist_id, observed.provider_snapshot_id
    )
    INSERT INTO provider_playlist_revisions
        (provider_playlist_id, provider_revision, content_sha256,
         item_count, first_observed_at, last_observed_at)
    SELECT provider_playlist_id, provider_snapshot_id, content_sha256,
           item_count, v_captured_at, v_captured_at
      FROM playlist_hashes
    ON CONFLICT (provider_playlist_id, content_sha256) DO UPDATE SET
        provider_revision = COALESCE(EXCLUDED.provider_revision,
                                     provider_playlist_revisions.provider_revision),
        last_observed_at = GREATEST(provider_playlist_revisions.last_observed_at,
                                   EXCLUDED.last_observed_at);

    INSERT INTO provider_playlist_revision_tracks
        (revision_id, provider_track_id, position, added_at, metadata)
    SELECT revision.id, membership.provider_track_id, membership.position,
           membership.added_at, membership.metadata
      FROM provider_playlist_snapshots observed
      JOIN provider_playlist_tracks membership
        ON membership.snapshot_id = observed.snapshot_id
       AND membership.provider_playlist_id = observed.provider_playlist_id
      JOIN provider_tracks track ON track.id = membership.provider_track_id
      JOIN provider_playlist_revisions revision
        ON revision.provider_playlist_id = observed.provider_playlist_id
       AND revision.content_sha256 = (
           SELECT encode(sha256(convert_to(COALESCE(string_agg(
               all_membership.position::text || ':' || all_track.provider || ':' ||
               all_track.provider_track_id, E'\n'
               ORDER BY all_membership.position, all_track.provider_track_id
           ), ''), 'UTF8')), 'hex')
             FROM provider_playlist_tracks all_membership
             JOIN provider_tracks all_track
               ON all_track.id = all_membership.provider_track_id
            WHERE all_membership.snapshot_id = p_snapshot_id
              AND all_membership.provider_playlist_id = observed.provider_playlist_id
       )
     WHERE observed.snapshot_id = p_snapshot_id
    ON CONFLICT (revision_id, position) DO NOTHING;

    SELECT encode(sha256(convert_to(COALESCE(string_agg(
               saved.position::text || ':' || track.provider || ':' ||
               track.provider_track_id || ':' || COALESCE(saved.saved_at::text, ''),
               E'\n' ORDER BY saved.position, track.provider_track_id
           ), ''), 'UTF8')), 'hex')
      INTO v_saved_track_hash
      FROM provider_saved_tracks saved
      JOIN provider_tracks track ON track.id = saved.provider_track_id
     WHERE saved.snapshot_id = p_snapshot_id;

    INSERT INTO provider_saved_track_revisions
        (provider_account_id, content_sha256, item_count,
         first_observed_at, last_observed_at)
    SELECT p_provider_account_id, v_saved_track_hash, count(*)::integer,
           v_captured_at, v_captured_at
      FROM provider_saved_tracks WHERE snapshot_id = p_snapshot_id
    ON CONFLICT (provider_account_id, content_sha256) DO UPDATE SET
        last_observed_at = GREATEST(provider_saved_track_revisions.last_observed_at,
                                   EXCLUDED.last_observed_at)
    RETURNING id INTO v_saved_track_revision_id;

    INSERT INTO provider_saved_track_revision_tracks
        (revision_id, provider_track_id, position, saved_at, metadata)
    SELECT v_saved_track_revision_id, provider_track_id, position, saved_at, metadata
      FROM provider_saved_tracks WHERE snapshot_id = p_snapshot_id
    ON CONFLICT (revision_id, position) DO NOTHING;

    WITH album_lines AS (
        SELECT 'album:' || album.position::text || ':' ||
               provider_album.provider || ':' || provider_album.provider_album_id || ':' ||
               COALESCE(album.saved_at::text, '') AS line,
               0 AS line_kind, album.position AS first_position, 0 AS second_position
          FROM provider_saved_albums album
          JOIN provider_albums provider_album ON provider_album.id = album.provider_album_id
         WHERE album.snapshot_id = p_snapshot_id
        UNION ALL
        SELECT 'track:' || provider_album.provider || ':' ||
               provider_album.provider_album_id || ':' || track.position::text || ':' ||
               provider_track.provider || ':' || provider_track.provider_track_id,
               1, album.position, track.position
          FROM provider_saved_album_tracks track
          JOIN provider_saved_albums album
            ON album.snapshot_id = track.snapshot_id
           AND album.provider_album_id = track.provider_album_id
          JOIN provider_albums provider_album ON provider_album.id = track.provider_album_id
          JOIN provider_tracks provider_track ON provider_track.id = track.provider_track_id
         WHERE track.snapshot_id = p_snapshot_id
    )
    SELECT encode(sha256(convert_to(COALESCE(string_agg(
               line, E'\n' ORDER BY first_position, line_kind, second_position, line
           ), ''), 'UTF8')), 'hex')
      INTO v_saved_album_hash FROM album_lines;

    INSERT INTO provider_saved_album_revisions
        (provider_account_id, content_sha256, album_count, track_count,
         first_observed_at, last_observed_at)
    VALUES (
        p_provider_account_id, v_saved_album_hash,
        (SELECT count(*)::integer FROM provider_saved_albums
          WHERE snapshot_id = p_snapshot_id),
        (SELECT count(*)::integer FROM provider_saved_album_tracks
          WHERE snapshot_id = p_snapshot_id),
        v_captured_at, v_captured_at
    )
    ON CONFLICT (provider_account_id, content_sha256) DO UPDATE SET
        last_observed_at = GREATEST(provider_saved_album_revisions.last_observed_at,
                                   EXCLUDED.last_observed_at)
    RETURNING id INTO v_saved_album_revision_id;

    INSERT INTO provider_saved_album_revision_albums
        (revision_id, provider_album_id, position, saved_at, metadata)
    SELECT v_saved_album_revision_id, provider_album_id, position, saved_at, metadata
      FROM provider_saved_albums WHERE snapshot_id = p_snapshot_id
    ON CONFLICT (revision_id, position) DO NOTHING;

    INSERT INTO provider_saved_album_revision_tracks
        (revision_id, provider_album_id, provider_track_id, position, metadata)
    SELECT v_saved_album_revision_id, provider_album_id,
           provider_track_id, position, metadata
      FROM provider_saved_album_tracks WHERE snapshot_id = p_snapshot_id
    ON CONFLICT (revision_id, provider_album_id, position) DO NOTHING;

    WITH playlist_state AS (
        SELECT provider_playlist.provider_playlist_id,
               revision.content_sha256
          FROM provider_playlist_snapshots observed
          JOIN provider_playlists provider_playlist
            ON provider_playlist.id = observed.provider_playlist_id
          JOIN provider_playlist_revisions revision
            ON revision.provider_playlist_id = observed.provider_playlist_id
           AND revision.content_sha256 = (
               SELECT encode(sha256(convert_to(COALESCE(string_agg(
                   membership.position::text || ':' || track.provider || ':' ||
                   track.provider_track_id, E'\n'
                   ORDER BY membership.position, track.provider_track_id
               ), ''), 'UTF8')), 'hex')
                 FROM provider_playlist_tracks membership
                 JOIN provider_tracks track ON track.id = membership.provider_track_id
                WHERE membership.snapshot_id = p_snapshot_id
                  AND membership.provider_playlist_id = observed.provider_playlist_id
           )
         WHERE observed.snapshot_id = p_snapshot_id
    ), state_lines AS (
        SELECT 'playlist:' || provider_playlist_id || ':' || content_sha256 AS line
          FROM playlist_state
        UNION ALL SELECT 'saved_tracks:' || v_saved_track_hash
        UNION ALL SELECT 'saved_albums:' || v_saved_album_hash
    )
    SELECT encode(sha256(convert_to(string_agg(line, E'\n' ORDER BY line), 'UTF8')), 'hex')
      INTO v_state_hash FROM state_lines;

    INSERT INTO provider_current_inventories
        (provider_account_id, provider, source_snapshot_id,
         saved_track_revision_id, saved_album_revision_id,
         state_sha256, captured_at, updated_at)
    VALUES (p_provider_account_id, v_provider, p_snapshot_id,
            v_saved_track_revision_id, v_saved_album_revision_id,
            v_state_hash, v_captured_at, now())
    ON CONFLICT (provider_account_id) DO UPDATE SET
        provider = EXCLUDED.provider,
        source_snapshot_id = EXCLUDED.source_snapshot_id,
        saved_track_revision_id = EXCLUDED.saved_track_revision_id,
        saved_album_revision_id = EXCLUDED.saved_album_revision_id,
        state_sha256 = EXCLUDED.state_sha256,
        captured_at = EXCLUDED.captured_at,
        updated_at = now();

    DELETE FROM provider_current_playlists
     WHERE provider_account_id = p_provider_account_id;

    INSERT INTO provider_current_playlists
        (provider_account_id, provider_playlist_id, revision_id,
         name, description, public, collaborative,
         provider_revision, reported_item_count, metadata)
    SELECT p_provider_account_id, observed.provider_playlist_id, revision.id,
           observed.name, observed.description, observed.public,
           observed.collaborative, observed.provider_snapshot_id,
           observed.total_items, observed.metadata
      FROM provider_playlist_snapshots observed
      JOIN provider_playlist_revisions revision
        ON revision.provider_playlist_id = observed.provider_playlist_id
       AND revision.content_sha256 = (
           SELECT encode(sha256(convert_to(COALESCE(string_agg(
               membership.position::text || ':' || track.provider || ':' ||
               track.provider_track_id, E'\n'
               ORDER BY membership.position, track.provider_track_id
           ), ''), 'UTF8')), 'hex')
             FROM provider_playlist_tracks membership
             JOIN provider_tracks track ON track.id = membership.provider_track_id
            WHERE membership.snapshot_id = p_snapshot_id
              AND membership.provider_playlist_id = observed.provider_playlist_id
       )
     WHERE observed.snapshot_id = p_snapshot_id;
END;
$$;

COMMENT ON FUNCTION materialize_provider_current_state_v2(UUID, UUID) IS
    'Transactionally replaces one account current provider state while reusing immutable content-addressed playlist and saved-surface revisions.';

WITH latest_successful AS (
    SELECT DISTINCT ON (run.provider_account_id)
           run.provider_account_id, run.snapshot_id
      FROM provider_import_runs run
     WHERE run.status = 'succeeded' AND run.snapshot_id IS NOT NULL
     ORDER BY run.provider_account_id, run.finished_at DESC NULLS LAST, run.id DESC
)
SELECT materialize_provider_current_state_v2(provider_account_id, snapshot_id)
  FROM latest_successful;

COMMENT ON TABLE provider_current_inventories IS
    'One transactionally replaceable current provider inventory pointer per account; legacy source_snapshot_id is transitional and may become NULL after cutover.';
COMMENT ON TABLE provider_playlist_revisions IS
    'Content-addressed immutable playlist bodies reused across unchanged provider pulls.';
COMMENT ON TABLE provider_inventory_checkpoints IS
    'Compact bounded pre-apply or named baselines; durable receipts reference these instead of complete routine provider snapshots.';
COMMENT ON TABLE normalized_listening_events IS
    'Permanent typed listening evidence; repeated display metadata belongs on historical_provider_track_identities and raw archives remain outside PostgreSQL.';
