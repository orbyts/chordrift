# Chordrift database object catalog

This is the plain-language index to the production schema through migration
0050. The [domain map](../design/chordrift-database-domain-map.svg) shows how
the groups relate; this page explains every public table and view.

## How to read the schema

| Lifecycle | Meaning |
| --- | --- |
| **Identity/current** | Stable identity or the current provider pointer. Required for normal operation. |
| **Evidence** | Durable user/provider facts that should not be silently discarded. |
| **Intent** | User-approved Chordrift meaning, policy, or placement. |
| **Audit** | Plans, approvals, applies, verification, recovery, and migration receipts. Retention may be compacted only with an explicit policy. |
| **Derived** | Rebuildable statistics, embeddings, enrichment, signals, or clusters. Old generations may eventually expire. |
| **Staging** | Transaction-local import work. These tables should be empty after a successful pull. |
| **Hosted** | Web/remote-client identity, encrypted credential, or durable job state. |
| **View** | A read-only SQL projection. These are the simplest raw SQL entry points. |

The number of tables is not the main storage cost. Most empty tables occupy
only a few pages. On 2026-08-31, the 113 tables occupied about 192 MiB in
relations; the complete application database was about 195 MiB. Seven views
store no rows.

## Simple read/query surfaces

Prefer the typed Chordrift CLI/service queries for application behavior. For
human SQL inspection, these seven views hide the revision-pointer joins:

| View | What a simple `SELECT` returns |
| --- | --- |
| `current_spotify_playlists` | Current Spotify playlist headers, counts, roles, and policy labels. |
| `provider_observed_playlists` | Current/protected provider playlist observations reconstructed from database-v2 revisions. |
| `provider_observed_playlist_tracks` | Current ordered track IDs for observed playlists. |
| `provider_observed_saved_tracks` | Current saved/liked track IDs and saved timestamps. |
| `provider_observed_saved_albums` | Current saved album IDs and saved timestamps. |
| `provider_observed_saved_album_tracks` | Tracks contained in currently observed saved albums. |
| `listening_evidence_events` | Normalized listening events with provider identity display metadata joined once. |

Useful read-model tables that are also straightforward to inspect are
`account_listening_track_statistics`, `account_track_statistics`,
`provider_current_playlists`, `provider_tracks`, and `excluded_tracks`.

Examples:

```sql
SELECT name, total_items, role
FROM current_spotify_playlists
ORDER BY name;

SELECT track_name, artist_name, play_count, last_played_at
FROM account_listening_track_statistics
ORDER BY play_count DESC, last_played_at DESC
LIMIT 50;

SELECT provider_track_id, played_at, ms_played, skipped
FROM listening_evidence_events
ORDER BY played_at DESC
LIMIT 100;

SELECT track_id, excluded_at, exclusion_reason, restored_at
FROM excluded_tracks
ORDER BY excluded_at DESC;
```

Raw revision, plan, apply, and verification tables are not simple current-state
queries. Reading one row without its generation/checkpoint context can produce
the wrong product conclusion.

## Schema and migration receipts

| Object | Lifecycle | What it stores |
| --- | --- | --- |
| `_sqlx_migrations` | Audit | Applied migration versions and checksums used by the Rust migrator. |
| `database_v2_migration_runs` | Audit | Exact-confirmed database-v2 migration plans and receipts. |
| `database_v2_cleanup_runs` | Audit | Exact-confirmed cleanup receipts proving legacy duplicate bodies were removed safely. |

## Product identity, authorization, and hosted operations

| Object | Lifecycle | What it stores |
| --- | --- | --- |
| `chordrift_accounts` | Identity/current | Provider-neutral Chordrift tenant/account identity. |
| `product_subjects` | Hosted | Stable Chordrift user subjects and active/suspended/closed status. |
| `product_external_identities` | Hosted | Verified Auth0/Google issuer-and-subject bindings; no OAuth token. |
| `chordrift_account_memberships` | Hosted | Subject-to-account ownership/membership and revocation state. |
| `product_sessions` | Hosted | Expiring/revocable Chordrift sessions using token digests only. |
| `provider_accounts` | Identity/current | Stable Spotify/provider account connection identity and account label. |
| `provider_credential_vault` | Hosted | Versioned encrypted provider refresh credentials, nonce, key selector, and revocation evidence; never plaintext or encryption keys. |
| `provider_capability_observations` | Evidence | Immutable observations of which provider/evidence capabilities were available for an account. |
| `service_operations` | Hosted/audit | Durable typed web/remote commands, state, retry, lease, cancellation, and idempotency metadata. |
| `service_operation_events` | Hosted/audit | Ordered append-only progress/state events for durable operations. |

## Canonical music and provider identities

| Object | Lifecycle | What it stores |
| --- | --- | --- |
| `tracks` | Identity/current | Provider-neutral canonical track identity. |
| `artists` | Identity/current | Canonical artist identity. |
| `albums` | Identity/current | Canonical album identity. |
| `track_artists` | Identity/current | Ordered track-to-artist credits. |
| `provider_tracks` | Identity/current | Spotify/provider track IDs, metadata, availability, and canonical links. |
| `provider_artists` | Identity/current | Spotify/provider artist identities and metadata. |
| `provider_albums` | Identity/current | Spotify/provider album identities and metadata. |
| `provider_playlists` | Identity/current | Stable Spotify/provider playlist IDs and provider-level metadata across observations. |
| `track_matches` | Evidence | Reviewed or inferred provider-track-to-canonical-track matches. |
| `historical_provider_track_identities` | Evidence | Lightweight identities and display metadata seen in long-term listening archives. |
| `track_artist_area_resolutions` | Derived/evidence | Cached artist-origin/area resolution used by enrichment. |

## Provider observation and current inventory

| Object | Lifecycle | What it stores |
| --- | --- | --- |
| `provider_current_inventories` | Identity/current | The one current inventory pointer per provider account. |
| `provider_current_playlists` | Identity/current | Current playlist headers pointing at immutable playlist revisions. |
| `provider_import_runs` | Audit | One provider import attempt, timing, status, and counts. |
| `provider_inventory_observations` | Audit | Lightweight completed observation receipts. |
| `provider_inventory_checkpoints` | Audit | Compact named/pre-apply baselines referenced by plans and receipts. |
| `provider_inventory_checkpoint_playlists` | Audit | Playlist revision pointers included in a checkpoint. |
| `provider_inventory_checkpoint_saved_surfaces` | Audit | Saved-track/album revision pointers included in a checkpoint. |
| `provider_inventory_import_playlists` | Staging | Playlist headers being assembled inside the current import transaction. |
| `provider_inventory_import_playlist_tracks` | Staging | Playlist membership being assembled inside the current import transaction. |
| `provider_inventory_import_saved_tracks` | Staging | Saved-track membership being assembled inside the current import transaction. |
| `provider_inventory_import_saved_albums` | Staging | Saved-album membership being assembled inside the current import transaction. |
| `provider_inventory_import_saved_album_tracks` | Staging | Saved-album track membership being assembled inside the current import transaction. |
| `provider_playlist_revisions` | Evidence/current | Content-addressed immutable playlist bodies reused across unchanged pulls. |
| `provider_playlist_revision_tracks` | Evidence/current | Exact ordered track membership for each unique provider playlist revision. |
| `provider_saved_track_revisions` | Evidence/current | Content-addressed saved/liked-track surface revisions. |
| `provider_saved_track_revision_tracks` | Evidence/current | Track membership and saved time for each saved-track revision. |
| `provider_saved_album_revisions` | Evidence/current | Content-addressed saved-album surface revisions. |
| `provider_saved_album_revision_albums` | Evidence/current | Saved album membership and order for each revision. |
| `provider_saved_album_revision_tracks` | Evidence/current | Track membership within saved albums for each revision. |
| `spotify_recent_play_syncs` | Audit | Bounded recent-play polling cursors, windows, and results. |
| `current_spotify_playlists` | View | Current Spotify playlist headers plus Chordrift policy labels. |
| `provider_observed_playlists` | View | Reconstructed current provider playlist headers. |
| `provider_observed_playlist_tracks` | View | Reconstructed current ordered provider playlist membership. |
| `provider_observed_saved_tracks` | View | Reconstructed current saved-track surface. |
| `provider_observed_saved_albums` | View | Reconstructed current saved-album surface. |
| `provider_observed_saved_album_tracks` | View | Reconstructed tracks within current saved albums. |

## Playlist intent, placement, and verification

| Object | Lifecycle | What it stores |
| --- | --- | --- |
| `provider_account_library_policies` | Intent | Account-wide provider library policy defaults. |
| `provider_account_playlists` | Identity/current | Provider playlists known to the account and their Chordrift role/relationship. |
| `playlist_concepts` | Intent | Stable conceptual identity for a managed playlist across generations and names. |
| `playlist_generations` | Intent/audit | One proposed/reviewed playlist-library generation. |
| `playlists` | Intent | Versioned playlist definitions inside generations. |
| `playlist_tracks` | Intent | Ordered proposed/approved playlist membership across generations. This is historical, not one current list. |
| `playlist_name_revisions` | Intent/audit | Reviewed poetic-name revisions and naming provenance. |
| `track_playlist_assignment_revisions` | Intent | Explainable canonical placement/reclassification decisions over time. |
| `excluded_tracks` | Intent | Reversible account-level exclusion state and restoration history. |
| `reevaluation_events` | Evidence/audit | Historical entry/exit observations for the retired Re-evaluate workflow. |
| `routing_surfaces` | Intent | Intake/review queues such as Inbox or From Friends and their clear behavior. |
| `managed_playlist_verifications` | Audit | Immutable proof that one managed provider playlist matched an approved state. |
| `managed_playlist_verified_tracks` | Audit/evidence | Exact verified membership baseline used to distinguish later removals from stale intent. |

## External playlist references and cleanup

| Object | Lifecycle | What it stores |
| --- | --- | --- |
| `external_playlist_bookmarks` | Intent | Account-scoped references to externally owned playlists; never canonical managed output. |
| `external_playlist_bookmark_snapshots` | Evidence | Immutable metadata observations captured during provider pulls. |
| `external_playlist_bookmark_tracks` | Evidence | Track membership retained for bookmarked external snapshots. |
| `external_playlist_bookmark_refreshes` | Audit | Explicit refresh attempts for one bookmark. |
| `external_playlist_bookmark_refresh_tracks` | Evidence | Readable track metadata obtained by an explicit bookmark refresh. |
| `external_playlist_cleanup_batches` | Audit | Reviewed/approved external-playlist relationship cleanup batches. |
| `external_playlist_cleanup_items` | Audit | Exact external playlist IDs and snapshot signatures in a cleanup batch. |

## Provider-neutral collections, surfaces, recipes, and Spins

| Object | Lifecycle | What it stores |
| --- | --- | --- |
| `library_collections` | Intent | Provider-neutral, overlapping library collections. |
| `collection_relationships` | Intent | Parent/related collection structure. |
| `collection_rule_revisions` | Intent/audit | Versioned collection inclusion/exclusion rules. |
| `playlist_surfaces` | Intent | User-visible playlist/intake surface intent independent of provider IDs. |
| `playlist_surface_provider_links` | Identity/current | Links a provider-neutral surface to its Spotify/provider playlist. |
| `playlist_track_directives` | Intent | Per-track keep/clear/preserve decisions for a surface, including Liked Songs disposition. |
| `playlist_recipes` | Intent | Stable recipe identity. |
| `playlist_recipe_revisions` | Intent/audit | Immutable versioned recipe documents. |
| `playlist_recipe_dependencies` | Intent | Normalized collection/capability inputs required by a recipe revision. |
| `playlist_spins` | Intent/audit | Deterministic provider-free Spin identity and input fingerprint. |
| `playlist_spin_tracks` | Intent | Exact selected/ordered tracks and reasons for a Spin. |
| `playlist_spin_publications` | Audit | Link from an approved Spin/surface to the plan-readiness-apply-verification chain. |
| `onboarding_sessions` | Audit | Provider-read-only onboarding checkpoint and evidence boundary. |

## Synchronization plans, authorization, and receipts

| Object | Lifecycle | What it stores |
| --- | --- | --- |
| `sync_runs` | Audit | Immutable dry-run plan identity, origin, phase, fingerprints, and overall status. |
| `sync_operations` | Audit | Exact ordered effects proposed by a synchronization plan. |
| `sync_readiness_assessments` | Audit | Read-only assessment of whether one exact plan/phase may be applied. |
| `sync_readiness_checks` | Audit | Individual scope, freshness, ordering, recovery, retry, and convergence checks. |
| `sync_apply_runs` | Audit | Explicitly confirmed resumable execution of one ready plan phase. |
| `sync_apply_operations` | Audit | Per-effect provider execution result and idempotent replay evidence. |
| `sync_apply_playlist_targets` | Audit | Provider playlist IDs resolved/created during apply so retries target the same surface. |
| `sync_retirement_approvals` | Audit | Exact durable authorization for reviewed legacy-retirement effects. |

## Listening evidence

| Object | Lifecycle | What it stores |
| --- | --- | --- |
| `listening_evidence_imports` | Evidence/audit | One immutable listening-history archive import, its hash, status, and coverage. |
| `listening_evidence_source_files` | Evidence/audit | Source file paths/hashes and import disposition. Raw archives remain outside PostgreSQL. |
| `normalized_listening_events` | Evidence | Permanent typed play events: time, duration, skip/completion evidence, context, provider identity, and provenance. |
| `listening_evidence_events` | View | Normalized events joined to provider identity display metadata for reads. |

## Classification, enrichment, and model evidence

| Object | Lifecycle | What it stores |
| --- | --- | --- |
| `track_classification_batches` | Intent/audit | Immutable human-review classification batch metadata. |
| `track_classification_batch_entries` | Intent | Exact tracks and proposed labels included in a review batch. |
| `track_classification_revisions` | Intent/evidence | Private account-scoped accepted/corrected track classification facts. |
| `enrichment_runs` | Derived/audit | One external/semantic enrichment generation and its inputs/status. |
| `track_enrichment_lookups` | Derived | Cached lookup attempts and provider/source responses. |
| `track_enrichment_matches` | Derived/evidence | Selected enrichment match and confidence/provenance. |
| `model_inference_imports` | Derived/audit | Imported model inference artifact metadata and version. |
| `track_model_facts` | Derived/evidence | Normalized factual outputs supplied by a model artifact. |
| `track_model_inferences` | Derived | Per-track predicted labels/scores from a model version. |
| `track_semantic_facts` | Derived/evidence | Independent semantic facts such as genre/language/region descriptors with provenance. |

## Statistics, signals, embeddings, and clusters

| Object | Lifecycle | What it stores |
| --- | --- | --- |
| `account_analysis_state` | Derived/current | Per-account pointer to the latest completed analysis generations. |
| `account_listening_track_statistics` | Derived | Long-term normalized play/skip/count/recency aggregates with display metadata. |
| `account_track_statistics` | Derived | Current account/library statistics used by clustering and playlist logic. |
| `account_track_signals` | Derived | Versioned personal preference/lifecycle signal features. |
| `signal_generations` | Derived/audit | Algorithm/version/input identity for account signal generation. |
| `track_statistics` | Derived | Older/general track statistics cache; currently empty. |
| `embedding_generations` | Derived/audit | Embedding algorithm/model/input generation metadata. |
| `track_embeddings` | Derived | Provider-neutral/base track vectors; currently empty in the live database. |
| `account_track_embeddings` | Derived | Personalized or account-scoped embedding vectors/features. |
| `cluster_generations` | Derived/audit | Clustering algorithm/version/input generation metadata. |
| `clusters` | Derived | Cluster identities and centroids/labels for one generation. |
| `cluster_tracks` | Derived | Track membership, distance, and rank within clusters. |
| `track_collection_membership_revisions` | Intent/derived | Versioned learned or reviewed track-to-collection membership; currently empty. |

## Artwork provenance

| Object | Lifecycle | What it stores |
| --- | --- | --- |
| `playlist_artwork_batches` | Audit | Immutable local cover review batches and approval state. |
| `playlist_artwork_artifacts` | Audit/evidence | Cover identity, hash, dimensions, and provenance. Image bytes stay in the repository/artifact tree, not Neon. |

## What currently consumes space

Measured on the canonical database on 2026-08-31:

| Domain | Relation size | Share |
| --- | ---: | ---: |
| Listening evidence | 85 MiB | 46.2% |
| Playlist intent and verified baselines | 53 MiB | 29.2% |
| Derived/classification caches | 16 MiB | 8.5% |
| Provider identities and observations | 14 MiB | 7.4% |
| Sync/migration audit | 12 MiB | 6.5% |
| Canonical music catalog | 2.0 MiB | 1.1% |
| Artwork provenance | 1.2 MiB | 0.7% |
| Product identity/service tables | 0.35 MiB | 0.2% |
| Schema metadata/other | 0.42 MiB | 0.2% |

Neon's approximately 228.24 MB branch figure is expected: the application
database is 204,226,560 bytes, while PostgreSQL's `postgres`, `template0`, and
`template1` databases add about 23.4 MB before small Neon/control-plane
rounding. The database is not secretly holding another music-library copy.

The storage is currently justified, but not every historical row must be kept
forever. The first safe optimization target is a designed retention/compaction
policy for old `playlist_tracks`, `managed_playlist_verified_tracks`, and sync
receipts that preserves current anchors, user intent, exclusions, exact apply
receipts, and recovery proofs. Listening evidence should remain durable;
derived generations can be regenerated and are the second retention target.
Do not drop empty product tables merely to save space: their combined cost is
small and they define the shipped schema contract.
