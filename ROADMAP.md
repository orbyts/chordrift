# Roadmap

Chordrift will be developed in small, auditable milestones. Neon is the source
of truth throughout, while Spotify and Apple Music remain provider adapters.

The Spotify downloadable history archive is an optional, independently
importable enrichment source. No other milestone is blocked on receiving it.

## Discovery and orchestration model

Chordrift will become the canonical orchestrator of playlists, while song
discovery remains native to each streaming platform. New provider playlists can
act as discovery inboxes: Chordrift ingests their tracks, identifies songs
already represented in the canonical library, and incorporates only the new
material into its analysis. Repeated runs should leave those inbox surfaces
clean and ready for further discovery without losing any tracks.

Existing provider playlists and their names remain intact until Chordrift has
clustered every track into an inspectable proposed structure and the user has
approved the LLM-suggested names and organization. Retiring an old or inbox
playlist must be an explicit, auditable synchronization operation performed
only after all of its tracks are accounted for in approved canonical playlists.

## v0.0.0 — Namespace reservation

Reserve the crate and repository names with a minimal, dependency-free package.

## v0.0.1 — Project skeleton and Storexa

Add configuration, CLI boundaries, Storexa-backed Neon access, migrations, and
the canonical schema. Provide version and database-status commands.

Status: complete.

## v0.0.2 — Spotify read-only inventory

Authenticate with Spotify and snapshot playlists, ordered memberships, saved
tracks, and provider metadata without remote mutations.

Status: complete.

## v0.0.3 — Canonical model and playlist analysis

Normalize tracks into provider-independent identities and report playlist
overlap, duplicate memberships, and library statistics. Add an incremental
Spotify-to-Neon pull that reconciles removals, keeps account identity dynamic,
and refreshes derived state. Persist account-scoped observed, discovery-inbox,
and managed playlist roles plus explicit drift authority, without remote
provider mutations.

Status: complete.

## v0.0.4 — Optional listening-history enrichment

Import Spotify history exports independently, preserve matched and unmatched
events, and derive play-count, duration, recency, and skip statistics. The rest
of Chordrift remains usable before or without this data.

Add a basic read-only query surface for accounts and playlists, ordered songs
within a playlist, canonical analysis, listening-history summaries, and
per-track listening statistics. These commands should make it possible to
verify that Neon is clean and current without writing SQL. Preserve duplicate
entries and expose stable provider IDs whenever a mutable name is ambiguous.

Archive ingestion uses a Git-ignored per-account inbox and collision-safe local
archive. Imports are content-addressed and event-deduplicated so periodic
overlapping exports can be added safely. Raw IP addresses and unrelated account
PII are excluded from the canonical database. Neon remains authoritative; the
unchanged local ZIP archive is a disaster-recovery and future reprocessing
source that can rebuild enrichment without requesting another Spotify export.

Status: complete.

## v0.0.5 — Apple Music matching

Add Apple Music authentication and ISRC-first catalog matching with scored
metadata fallbacks and unresolved-match reporting. Keep operations read-only.

## v0.0.6 — Embeddings

Build versioned personal embeddings from playlist co-occurrence, artist and
metadata relationships, historical names, and any available listening signals.

## v0.0.7 — Vibe clustering

Create reproducible cluster generations with stable identities,
representatives, statistics, and support for unassigned tracks.

## v0.0.8 — Naming and proposed library

Generate names, descriptions, and semantic tags, then expose a complete,
inspectable, non-destructive proposed playlist structure. Require user approval
of generated names and organization, and prove that every track from each
legacy or discovery playlist is represented before proposing its retirement.

## v0.0.9 — Full dry-run synchronization

Plan idempotent Spotify and Apple Music diffs, unresolved matches, and Apple
Music Spatial Audio variants without mutating either service. Include discovery
inbox ingestion, cross-playlist duplicate removal, and explicit retirement plans
for legacy and consumed inbox playlists, with track-preservation checks and no
implicit deletions.

## v0.1.0 — Canonical music library

Synchronize approved canonical playlists from Neon to both providers, create
Spatial Audio companions in Apple Music, retain provenance and operation
history, and converge to zero changes on repeated runs. Remove legacy or
consumed discovery playlists only as separately approved operations after their
replacement playlists are published and verified.
