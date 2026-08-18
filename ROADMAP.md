# Roadmap

Chordrift will be developed in small, auditable milestones. Neon is the source
of truth throughout, while Spotify and Apple Music remain provider adapters.

The Spotify downloadable history archive is an optional, independently
importable enrichment source. No other milestone is blocked on receiving it.

## v0.0.0 — Namespace reservation

Reserve the crate and repository names with a minimal, dependency-free package.

## v0.0.1 — Project skeleton and Storexa

Add configuration, CLI boundaries, Storexa-backed Neon access, migrations, and
the canonical schema. Provide version and database-status commands.

Status: complete.

## v0.0.2 — Spotify read-only inventory

Authenticate with Spotify and snapshot playlists, ordered memberships, saved
tracks, and provider metadata without remote mutations.

## v0.0.3 — Canonical model and playlist analysis

Normalize tracks into provider-independent identities and report playlist
overlap, duplicate memberships, and library statistics.

## v0.0.4 — Optional listening-history enrichment

Import Spotify history exports independently, preserve matched and unmatched
events, and derive play-count, duration, recency, and skip statistics. The rest
of Chordrift remains usable before or without this data.

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
inspectable, non-destructive proposed playlist structure.

## v0.0.9 — Full dry-run synchronization

Plan idempotent Spotify and Apple Music diffs, unresolved matches, and Apple
Music Spatial Audio variants without mutating either service.

## v0.1.0 — Canonical music library

Synchronize approved canonical playlists from Neon to both providers, create
Spatial Audio companions in Apple Music, retain provenance and operation
history, and converge to zero changes on repeated runs.
