CREATE TABLE track_artist_area_resolutions (
    track_id UUID NOT NULL REFERENCES tracks (id) ON DELETE CASCADE,
    source TEXT NOT NULL CHECK (btrim(source) <> ''),
    parser_version TEXT NOT NULL CHECK (btrim(parser_version) <> ''),
    match_parser_version TEXT NOT NULL CHECK (btrim(match_parser_version) <> ''),
    artist_mbid TEXT NOT NULL CHECK (btrim(artist_mbid) <> ''),
    artist_name TEXT NOT NULL CHECK (btrim(artist_name) <> ''),
    lookup_id UUID NOT NULL REFERENCES track_enrichment_lookups (id) ON DELETE RESTRICT,
    status TEXT NOT NULL CHECK (status IN ('resolved', 'unknown', 'error')),
    area_mbid TEXT,
    area_name TEXT,
    country_code TEXT,
    confidence DOUBLE PRECISION NOT NULL CHECK (confidence BETWEEN 0 AND 1),
    provenance JSONB NOT NULL DEFAULT '{}'::jsonb,
    resolved_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((status = 'resolved') = (area_mbid IS NOT NULL AND area_name IS NOT NULL)),
    CHECK ((area_mbid IS NULL) = (area_name IS NULL)),
    FOREIGN KEY (track_id, source, match_parser_version)
        REFERENCES track_enrichment_matches (track_id, source, parser_version)
        ON DELETE CASCADE,
    PRIMARY KEY (track_id, source, parser_version, artist_mbid)
);

CREATE INDEX track_artist_area_resolutions_artist_idx
    ON track_artist_area_resolutions (source, parser_version, artist_mbid);
CREATE INDEX track_artist_area_resolutions_status_idx
    ON track_artist_area_resolutions (source, parser_version, status);
