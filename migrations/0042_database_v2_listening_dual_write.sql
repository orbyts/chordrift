-- Keep normalized listening evidence current after an approved cutover. These
-- local database triggers make no provider requests and retain the legacy write
-- path during the observation window.

CREATE OR REPLACE FUNCTION sync_spotify_archive_import_v2()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    v_provider TEXT;
BEGIN
    SELECT provider INTO v_provider
      FROM provider_accounts WHERE id = NEW.provider_account_id;

    INSERT INTO listening_evidence_imports
        (id, provider_account_id, provider, archive_kind, archive_sha256,
         parser_version, source_filename, source_file_count, event_count,
         first_event_at, last_event_at, manifest, imported_at)
    VALUES (
        NEW.id, NEW.provider_account_id, v_provider, NEW.archive_kind,
        NEW.archive_sha256, 'legacy-history-v1', NEW.source_filename,
        NEW.source_files, NEW.events_imported, NEW.first_event_at,
        NEW.last_event_at,
        jsonb_build_object(
            'legacy_import_id', NEW.id,
            'events_seen', NEW.events_seen,
            'events_matched', NEW.events_matched,
            'events_ignored', NEW.events_ignored,
            'legacy_metadata', NEW.metadata,
            'member_hashes', 'unavailable; containing archive hash verified'
        ),
        NEW.imported_at
    )
    ON CONFLICT (provider_account_id, provider, archive_sha256) DO UPDATE SET
        source_file_count = EXCLUDED.source_file_count,
        event_count = EXCLUDED.event_count,
        first_event_at = EXCLUDED.first_event_at,
        last_event_at = EXCLUDED.last_event_at,
        manifest = EXCLUDED.manifest;
    RETURN NEW;
END;
$$;

CREATE TRIGGER spotify_archive_imports_v2_dual_write
AFTER INSERT OR UPDATE OF source_files, events_seen, events_imported,
    events_matched, events_ignored, first_event_at, last_event_at, metadata
ON spotify_archive_imports
FOR EACH ROW EXECUTE FUNCTION sync_spotify_archive_import_v2();

CREATE OR REPLACE FUNCTION sync_listening_event_v2()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    v_identity_id UUID;
    v_source_file_id UUID;
BEGIN
    IF NEW.media_type <> 'track' THEN
        RETURN NEW;
    END IF;
    IF NEW.provider_track_id IS NULL OR btrim(NEW.provider_track_id) = '' THEN
        RAISE EXCEPTION 'track listening event requires provider_track_id';
    END IF;
    IF NEW.source_kind = 'archive' AND NEW.source_import_id IS NULL THEN
        RAISE EXCEPTION 'archive listening event requires source_import_id';
    END IF;

    INSERT INTO historical_provider_track_identities
        (provider, provider_track_id, canonical_track_id, track_name,
         artist_name, album_name, first_observed_at, last_observed_at)
    VALUES (
        NEW.provider, NEW.provider_track_id, NEW.track_id,
        NEW.raw_metadata->>'track_name', NEW.raw_metadata->>'artist_name',
        NEW.raw_metadata->>'album_name', NEW.played_at, NEW.played_at
    )
    ON CONFLICT (provider, provider_track_id) DO UPDATE SET
        canonical_track_id = COALESCE(EXCLUDED.canonical_track_id,
                                      historical_provider_track_identities.canonical_track_id),
        track_name = COALESCE(EXCLUDED.track_name,
                              historical_provider_track_identities.track_name),
        artist_name = COALESCE(EXCLUDED.artist_name,
                               historical_provider_track_identities.artist_name),
        album_name = COALESCE(EXCLUDED.album_name,
                              historical_provider_track_identities.album_name),
        first_observed_at = LEAST(historical_provider_track_identities.first_observed_at,
                                  EXCLUDED.first_observed_at),
        last_observed_at = GREATEST(historical_provider_track_identities.last_observed_at,
                                   EXCLUDED.last_observed_at)
    RETURNING id INTO v_identity_id;

    IF NEW.source_kind = 'archive' AND NEW.source_file IS NOT NULL THEN
        INSERT INTO listening_evidence_source_files
            (import_id, source_path, content_sha256, event_count, hash_status)
        VALUES (NEW.source_import_id, NEW.source_file, NULL, 1,
                'archive_manifest_only')
        ON CONFLICT (import_id, source_path) DO UPDATE SET
            event_count = (
                SELECT count(*) FROM listening_events event
                 WHERE event.source_import_id = NEW.source_import_id
                   AND event.source_file = NEW.source_file
                   AND event.media_type = 'track'
            )
        RETURNING id INTO v_source_file_id;
    END IF;

    INSERT INTO normalized_listening_events
        (id, provider_account_id, historical_identity_id, source_import_id,
         source_file_id, source_kind, source_event_id, source_occurrence,
         played_at, ms_played, skipped, completed, completion_reason,
         context_uri, context_type, superseded_at, provider_extensions)
    VALUES (
        NEW.id, NEW.provider_account_id, v_identity_id, NEW.source_import_id,
        v_source_file_id, NEW.source_kind, NEW.source_event_id,
        NEW.source_occurrence, NEW.played_at, NEW.ms_played, NEW.skipped,
        CASE WHEN NEW.raw_metadata ? 'reason_end'
             THEN NEW.raw_metadata->>'reason_end' = 'trackdone' END,
        NEW.raw_metadata->>'reason_end', NEW.context_uri,
        NEW.raw_metadata->>'context_type', NEW.superseded_at, '{}'::jsonb
    )
    ON CONFLICT (id) DO UPDATE SET
        historical_identity_id = EXCLUDED.historical_identity_id,
        source_import_id = EXCLUDED.source_import_id,
        source_file_id = EXCLUDED.source_file_id,
        source_kind = EXCLUDED.source_kind,
        source_event_id = EXCLUDED.source_event_id,
        source_occurrence = EXCLUDED.source_occurrence,
        played_at = EXCLUDED.played_at,
        ms_played = EXCLUDED.ms_played,
        skipped = EXCLUDED.skipped,
        completed = EXCLUDED.completed,
        completion_reason = EXCLUDED.completion_reason,
        context_uri = EXCLUDED.context_uri,
        context_type = EXCLUDED.context_type,
        superseded_at = EXCLUDED.superseded_at;
    RETURN NEW;
END;
$$;

CREATE TRIGGER listening_events_v2_dual_write
AFTER INSERT OR UPDATE OF track_id, provider_track_id, source_import_id,
    source_file, source_kind, source_event_id, source_occurrence, played_at,
    ms_played, skipped, context_uri, raw_metadata, superseded_at
ON listening_events
FOR EACH ROW EXECUTE FUNCTION sync_listening_event_v2();

COMMENT ON FUNCTION sync_listening_event_v2() IS
    'Dual-writes typed normalized evidence while the legacy listening event table remains intact for rollback.';
