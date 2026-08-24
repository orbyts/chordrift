# Changelog

All notable changes to Chordrift are documented here.

## [Unreleased]

- Add immutable, idempotent Spotify dry-run plans bound to one approved
  proposal and one imported snapshot, with exact operation inspection and no
  provider requests or mutations.
- Separate publication, managed-drift reconciliation, verified inbox cleanup,
  and explicitly approved legacy retirement into ordered safety phases with
  track-preservation gates.
- Include missing stable `Inbox`, `From Friends`, and `Liked from Radio`
  containers in the plan without duplicating existing intake surfaces.
- Schedule deterministic, original, explicitly approved cover artwork for
  every canonical playlist in v0.0.9, before any v0.1.0 publication.
- Add a provider-neutral reversible Excluded Tracks ledger and report restores
  separately from ordinary additions and provider drift.
- Add immutable managed-playlist verification baselines so an expected user
  removal proposes an exclusion while an unexpected extra remains ordinary
  drift; allow cleanup gates to recognize durable exclusions as resolved track
  dispositions.

- Add model-neutral pretrained-audio inference artifacts and cache-first
  MusicBrainz recording, tag, release, and artist-area enrichment with strict
  provenance and no provider-audio acquisition.
- Add deterministic semantic-seeded vibe clusters with explicit unassigned
  tracks, immutable inputs, reproducibility hashes, and inspection commands.
- Add non-destructive proposed playlist generations with stable lineage-backed
  identities, per-source retirement coverage, and no Spotify writes.
- Add strict naming-context export and naming-result import with generator
  provenance, revision history, reserved-name validation, and explicit
  generation approval gated by complete naming and track coverage.
- Add a database-level current Spotify playlist projection so active queries
  expose only latest-snapshot names while immutable snapshots retain history.
- Add per-track inspection of proposal coverage gaps with Spotify identities
  and all contributing legacy/intake source playlists.
- Add stable manual semantic categories and reversible, audited track
  assignment/review decisions that replay into future proposal generations.
- Define the stable intake names `Inbox`, `From Friends`, and
  `Liked from Radio`, including their distinct user-facing meanings and safe
  retirement rules for obsolete utility playlists.

## [0.0.5] - 2026-08-24

- Add immutable, account-scoped semantic embedding generations with audit,
  status, and nearest-neighbor inspection commands.
- Keep listening, saved, rotation, discovery, intake, recommendation, and
  prompted-interest signals in a separately versioned behavioral generation.
- Add explicit playlist signal classes and clearing policies so provider-owned,
  intake, transport, ignored, semantic legacy, and canonical playlists cannot
  accidentally teach or clear one another.
- Generate a deterministic 1,024-dimensional semantic fallback from approved
  playlist, artist, album, and historical-name relationships while reserving
  canonical acoustic embeddings for lawful locally owned audio.
- Record the deferred Apple Music bootstrap, Spatial Audio workaround,
  provider tombstones, and reversible Excluded Tracks policy without enabling
  provider mutations.
- Document provenance-aware language, release-country, artist-region, mood,
  and sound enrichment as the prerequisite to clustering.

## [0.0.4] - 2026-08-24

- Add `playlists tracks` to inspect the ordered contents of a playlist's latest
  imported Neon snapshot by unambiguous name or stable Spotify ID.
- Add and link a canonical user-facing CLI guide with everyday sync and
  verification examples.
- Add privacy-conscious inspection and idempotent import of Spotify account-data
  and extended streaming-history ZIP archives.
- Preserve exact music playback timestamps, durations, skips, interaction
  reasons, platform/context flags, and Spotify track IDs while excluding IP
  addresses and account-profile PII.
- Add a Git-ignored, collision-safe local inbox/archive workflow that keeps
  Spotify's original `my_spotify_data.zip` filename.
- Add replay of retained local archives for Neon disaster recovery while
  keeping Neon authoritative during normal operation.
- Deduplicate cumulative future exports by stable core playback identity rather
  than archive or source-file boundaries.
- Derive account-scoped per-track event, meaningful-play, duration, skip,
  completion, and recency statistics; relink history after normal provider syncs.

## [0.0.3] - 2026-08-18

- Add `sync pull` to incrementally reconcile Spotify edits into Neon and refresh
  canonical analysis in one command.
- Preserve stable Spotify account and playlist identities independently from
  local account labels and mutable playlist names.
- Track account-scoped observed, discovery-inbox, and managed playlist roles,
  including provider-wins, Neon-wins, and manual drift policies.
- Mark historically known playlists absent when they disappear from the latest
  imported snapshot without deleting their history.
- Add aggregate library summaries, cross-playlist overlap reports, and
  within-playlist canonical duplicate reports.
- Keep v0.0.3 pull-only: role and drift policy are durable preparation for a
  later auditable dry-run/apply workflow and do not mutate Spotify.

## [0.0.2] - 2026-08-18

- Add Spotify Authorization Code with PKCE using read-only playlist and library
  scopes.
- Store account-scoped refresh tokens in macOS Passwords/Keychain and keep
  OAuth secrets out of Neon and shell initialization files.
- Add Spotify authorization, status, logout, and inventory-import commands.
- Snapshot owned and accessible collaborative playlists, ordered membership,
  duplicate entries, saved tracks, and provider metadata atomically in Neon.
- Preserve unavailable and unsupported item counts and report playlists skipped
  because of Spotify Development Mode access restrictions.
- Reuse unchanged playlist membership and saved-track inventories from Neon,
  reducing steady-state imports to the playlist index, changed playlists, and a
  one-page saved-library probe.
- Keep Spotify listening-history downloads optional for later play-count and
  listening-statistics enrichment.

## [0.0.1] - 2026-08-18

- Add the initial command-line application and `--version` support.
- Integrate Storexa 0.1.0 for Neon/PostgreSQL connections, health checks, and
  application-owned migration execution.
- Add `db status` for read-only health and migration diagnostics.
- Add `db migrate` for the embedded canonical schema.
- Establish canonical tracks, provider identities, immutable provider-library
  snapshots, listening events, embeddings, clusters, playlist generations,
  and synchronization audit records.
- Keep Spotify listening-history exports optional and independently importable.

## [0.0.0] - 2026-08-18

- Reserve the Chordrift crate and repository namespaces.

[Unreleased]: https://github.com/orbyts/chordrift/compare/v0.0.5...HEAD
[0.0.5]: https://github.com/orbyts/chordrift/compare/v0.0.4...v0.0.5
[0.0.4]: https://github.com/orbyts/chordrift/compare/v0.0.3...v0.0.4
[0.0.3]: https://github.com/orbyts/chordrift/compare/v0.0.2...v0.0.3
[0.0.2]: https://github.com/orbyts/chordrift/compare/v0.0.1...v0.0.2
[0.0.1]: https://github.com/orbyts/chordrift/compare/v0.0.0...v0.0.1
[0.0.0]: https://github.com/orbyts/chordrift/releases/tag/v0.0.0
