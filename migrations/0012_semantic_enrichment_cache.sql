CREATE TABLE enrichment_runs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider_account_id UUID NOT NULL REFERENCES provider_accounts (id) ON DELETE RESTRICT,
    source TEXT NOT NULL CHECK (btrim(source) <> ''),
    source_version TEXT NOT NULL CHECK (btrim(source_version) <> ''),
    status TEXT NOT NULL CHECK (status IN ('running', 'succeeded', 'failed')),
    tracks_considered INTEGER NOT NULL DEFAULT 0 CHECK (tracks_considered >= 0),
    requests_made INTEGER NOT NULL DEFAULT 0 CHECK (requests_made >= 0),
    cache_hits INTEGER NOT NULL DEFAULT 0 CHECK (cache_hits >= 0),
    matched_tracks INTEGER NOT NULL DEFAULT 0 CHECK (matched_tracks >= 0),
    ambiguous_tracks INTEGER NOT NULL DEFAULT 0 CHECK (ambiguous_tracks >= 0),
    unmatched_tracks INTEGER NOT NULL DEFAULT 0 CHECK (unmatched_tracks >= 0),
    error_tracks INTEGER NOT NULL DEFAULT 0 CHECK (error_tracks >= 0),
    facts_written INTEGER NOT NULL DEFAULT 0 CHECK (facts_written >= 0),
    parameters JSONB NOT NULL DEFAULT '{}'::jsonb,
    started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at TIMESTAMPTZ
);

CREATE FUNCTION account_track_is_eligible(p_account_id UUID, p_canonical_track_id UUID)
RETURNS BOOLEAN
LANGUAGE sql
STABLE
AS $$
    SELECT EXISTS (
        SELECT 1
        FROM provider_library_snapshots snapshot
        JOIN provider_playlist_tracks membership ON membership.snapshot_id = snapshot.id
        JOIN provider_tracks provider_track ON provider_track.id = membership.provider_track_id
        WHERE snapshot.id = (
            SELECT latest.id
            FROM provider_library_snapshots latest
            WHERE latest.provider_account_id = p_account_id
            ORDER BY latest.captured_at DESC, latest.id DESC
            LIMIT 1
        )
          AND provider_track.track_id = p_canonical_track_id
    ) OR EXISTS (
        SELECT 1
        FROM provider_library_snapshots snapshot
        JOIN provider_saved_tracks saved ON saved.snapshot_id = snapshot.id
        JOIN provider_tracks provider_track ON provider_track.id = saved.provider_track_id
        WHERE snapshot.id = (
            SELECT latest.id
            FROM provider_library_snapshots latest
            WHERE latest.provider_account_id = p_account_id
            ORDER BY latest.captured_at DESC, latest.id DESC
            LIMIT 1
        )
          AND provider_track.track_id = p_canonical_track_id
    ) OR EXISTS (
        SELECT 1
        FROM account_listening_track_statistics statistics
        WHERE statistics.provider_account_id = p_account_id
          AND statistics.track_id = p_canonical_track_id
    )
$$;

CREATE INDEX enrichment_runs_account_started_at_idx
    ON enrichment_runs (provider_account_id, source, started_at DESC);

CREATE TABLE track_enrichment_lookups (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    source TEXT NOT NULL CHECK (btrim(source) <> ''),
    api_version TEXT NOT NULL CHECK (btrim(api_version) <> ''),
    lookup_kind TEXT NOT NULL CHECK (lookup_kind IN ('isrc', 'recording', 'artist')),
    lookup_value TEXT NOT NULL CHECK (btrim(lookup_value) <> ''),
    outcome TEXT NOT NULL CHECK (outcome IN ('response', 'not_found', 'error')),
    http_status INTEGER CHECK (http_status IS NULL OR http_status BETWEEN 100 AND 599),
    response JSONB,
    response_sha256 TEXT,
    error_class TEXT,
    fetched_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    retry_after TIMESTAMPTZ,
    CHECK ((outcome = 'response') = (response IS NOT NULL)),
    UNIQUE (source, api_version, lookup_kind, lookup_value)
);

CREATE INDEX track_enrichment_lookups_retry_idx
    ON track_enrichment_lookups (source, retry_after)
    WHERE outcome = 'error';

CREATE TABLE track_enrichment_matches (
    track_id UUID NOT NULL REFERENCES tracks (id) ON DELETE CASCADE,
    source TEXT NOT NULL CHECK (btrim(source) <> ''),
    parser_version TEXT NOT NULL CHECK (btrim(parser_version) <> ''),
    lookup_id UUID NOT NULL REFERENCES track_enrichment_lookups (id) ON DELETE RESTRICT,
    status TEXT NOT NULL CHECK (status IN ('matched', 'ambiguous', 'unmatched', 'error')),
    source_entity_id TEXT,
    candidate_count INTEGER NOT NULL CHECK (candidate_count >= 0),
    confidence DOUBLE PRECISION CHECK (confidence IS NULL OR confidence BETWEEN 0 AND 1),
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    resolved_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((status = 'matched') = (source_entity_id IS NOT NULL)),
    PRIMARY KEY (track_id, source, parser_version)
);

CREATE INDEX track_enrichment_matches_status_idx
    ON track_enrichment_matches (source, status, resolved_at DESC);

CREATE TABLE track_semantic_facts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    track_id UUID NOT NULL REFERENCES tracks (id) ON DELETE CASCADE,
    match_track_id UUID NOT NULL,
    source TEXT NOT NULL,
    parser_version TEXT NOT NULL,
    source_entity_id TEXT NOT NULL CHECK (btrim(source_entity_id) <> ''),
    fact_kind TEXT NOT NULL CHECK (fact_kind IN (
        'genre', 'tag', 'mood', 'sound_descriptor',
        'release_language', 'release_country', 'artist_area'
    )),
    value TEXT NOT NULL CHECK (btrim(value) <> ''),
    normalized_value TEXT NOT NULL CHECK (btrim(normalized_value) <> ''),
    weight DOUBLE PRECISION NOT NULL DEFAULT 1 CHECK (weight >= 0),
    confidence DOUBLE PRECISION NOT NULL CHECK (confidence BETWEEN 0 AND 1),
    provenance JSONB NOT NULL DEFAULT '{}'::jsonb,
    observed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (track_id = match_track_id),
    FOREIGN KEY (match_track_id, source, parser_version)
        REFERENCES track_enrichment_matches (track_id, source, parser_version)
        ON DELETE CASCADE,
    UNIQUE (track_id, source, parser_version, source_entity_id, fact_kind, normalized_value)
);

CREATE INDEX track_semantic_facts_track_kind_idx
    ON track_semantic_facts (track_id, fact_kind);
CREATE INDEX track_semantic_facts_kind_value_idx
    ON track_semantic_facts (fact_kind, normalized_value);
