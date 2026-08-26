# Changelog

All notable changes to Chordrift are documented here.

## [Unreleased]

## [0.1.3] - 2026-08-26

- Prepare v0.1.3 as the database-v2 runtime: all ordinary reads use current
  content-addressed provider state and normalized listening evidence instead of
  duplicated provider snapshots or legacy event rows.
- Write Spotify archive and recent-play evidence directly to normalized imports,
  historical identities, source files, and typed listening events.
- Persist provider pulls through transient v2 import surfaces, reuse unchanged
  playlist and saved-library revisions, and leave import staging empty at commit.
- Add exact-confirmed `db compact cleanup plan/apply/verify` phases that preserve
  durable invariants and receipts while renaming provider staging tables and
  removing superseded database-v1 event/archive tables.
- Prove the clean runtime on fresh PostgreSQL 18 rehearsals, migrate the verified
  replacement Neon project, switch the persistent application connection, and
  retire the former project only after cleanup and observation gates pass.
- Preserve exact invariant parity, 149,314 normalized listening events, both
  archive manifests, 24 compact checkpoints, and all durable audit history while
  reducing the live database from 358,850,560 to 167,788,544 bytes.

## [0.1.2] - 2026-08-25

- Replace destination-specific review routes with one provider-owned,
  zero-signal `Re-evaluate` queue; record immutable entry/exit history, suppress
  accidental exclusion and source restoration while queued, export long queues
  through the standard classification worksheet, and clear an item only after
  a newer explicit assignment selects a different approved destination.
- Add exact-confirmed, coverage-gated legacy-route retirement whose Spotify
  container archival remains an immutable retirement-plan operation.
- Require complete approved proposals to emit an explicit, separately gated
  retirement for every omitted managed canonical playlist, preventing retired
  concepts such as `Monsoon Cinema` from surviving silently.
- Reconcile the approved 454-track South Asian review into `Dakshina Pulse`,
  `Uttara Glow`, and `Rasa Archive`; retire unavailable `Latika's Theme`, move
  global score material back to `Afterlight Score`, and reach a complete
  1,754-track proposal with zero unresolved or conflicting placements.
- Add the approved Drift Atlas v4 cover set for all 16 canonical and four intake
  surfaces, including new artwork for `Dakshina Pulse`, `Uttara Glow`, `Rasa
  Archive`, and the `Re-evaluate` queue.
- Mirror the provider-owned Re-evaluate queue through exact replacement without
  targeting the generated-membership partial uniqueness index.
- Verify approved canonical destinations independently of separately gated
  legacy containers, avoiding a circular verification/retirement dependency.
- Complete the live v0.1.2 Spotify reconciliation: publish all approved
  destinations and covers, clear 145 consumed Inbox entries, retire Monsoon
  Cinema and the three legacy routes, and reach a zero-operation plan.
- Opt Suhail's account into verified Liked Songs consumption, remove all 346
  supported saved tracks only after proving 345 canonical placements and one
  durable exclusion, verify zero supported saved tracks from Spotify, and
  converge again to a zero-operation plan.
- Derive PostgreSQL integration-test migration expectations from the embedded
  migration set so release CI cannot drift behind new schema migrations.
- Define the canonical/intake/generated-playlist product model, capability-aware
  recipe direction, permission-bounded agentic onboarding, quiet background
  operation, and the staged native-product roadmap toward v1.0.0.

- Add account-scoped `user_cohorts`, a complete classification/CSV glossary,
  Excel-ready A.R. Rahman examples, and stronger selected-account track
  isolation as the `0.1.2-dev.3` checkpoint.

- Add revisioned private collection, region, tradition, and language facts with
  atomic one-track/small-batch commands and an inert CSV review workflow.
- Render `tracks inspect` as a human-oriented interactive report, retain raw
  generation/provenance detail behind `--technical`, and expand interactive
  tables across the detected terminal width.
- Label this checkpoint `0.1.2-dev.2`; route reconciliation and the reviewed
  Monsoon Cinema split remain required before stable `0.1.2`.

- Add durable, zero-signal routing playlists for low-friction correction
  capture while listening. Route consumption into verified canonical
  destinations remains under development for v0.1.2.
- Avoid rewriting unchanged Spotify track metadata after a saved-library
  change, batch immutable saved-track membership in Neon, and expose TTY
  progress bars with plain redirected fallbacks.
- Render interactive playlist and song listings as compact colored tables while
  retaining complete tab-separated output for scripts and logs.

## [0.1.1] - 2026-08-24

- Add cursor-based Recently Played ingestion to normal Spotify pulls while
  retaining the lifetime extended-history archive as the authoritative source
  for durations, completions, and skips. Later archives supersede overlapping
  provisional API observations before statistics are rebuilt.
- Consolidate Spotify consent around one scope set and stop rewriting unchanged
  Keychain credentials during every command.
- Protect newly encountered user-owned playlists as `user_managed` by default.
  Add non-destructive retirement selection for named playlists, all playlists
  with explicit exceptions, or none; actual retirement keeps every existing
  coverage, approval, readiness, and destructive-apply gate.

- Add `tracks inspect` for one-command current placement, canonical assignment,
  source-playlist provenance, listening signals, embedding/cluster rationale,
  model facts, manual overrides, and exclusions.
- Add the `From Prompts` intake with prompted-interest semantics alongside
  `Inbox`, `From Friends`, and `Liked from Radio`.
- Add Drift Atlas v3 artwork for all canonical and intake surfaces,
  deterministic Spotify-scale lower-left Helvetica Neue labels, and preserved
  label-free masters for future Apple Music typography.
- Suppress already-succeeded identical artwork uploads for the same stable
  Spotify playlist, select only the newest approved artwork batch, and add
  `artwork update --playlist` for immutable one-cover update plans.
- Verify approved empty playlists and compare sparse proposal ordering keys as
  ordered track sequences after Spotify densifies provider positions.
- Document Spotify playlist folders as manual client-only presentation state;
  the Web API exposes neither folder structure nor custom folder covers.
- Define an account-scoped complete-library inventory from saved tracks and
  durable semantic, transport, intake, and canonical playlist history. Add
  `proposals inventory` and `proposals unresolved`, and block complete coverage
  unless every preserved track is placed or explicitly excluded.
- Keep listening history and provider-curated playlists as enrichment signals
  without silently importing every casually played track into the library.
- Add listening-session co-occurrence to personal embedding model v4, reducing
  complete-inventory tracks without useful vectors from 173 to 23 while
  retaining exact model/generation provenance.
- Preserve approved playlist identities through extension proposals, then add
  auditable direct-centroid, analytical-group-consensus, and explicit manual
  assignment paths. The personal proposal now represents all 1,711 preserved
  tracks exactly once with zero exclusions, unresolved tracks, or conflicts.
- Make manual assignment and `tracks inspect` understand transport-only and
  proposed-library tracks instead of limiting them to the current provider
  snapshot or last approved proposal.

## [0.1.0] - 2026-08-24

- Add a gated, resumable Spotify apply engine with exact assessment
  confirmation, separately acknowledged destructive phases, durable
  per-operation execution history, and post-pull convergence verification.
- Batch playlist additions and removals at Spotify's current limits, reconcile
  live membership before interrupted resumes, and use the February 2026
  `/items` and `/me/library` endpoints.
- Add approved cover uploads with deterministic local PNG-to-JPEG conversion,
  strict 256 KB payload enforcement, and explicit image-upload authorization.
- Add exact-plan retirement approval and require every canonical destination to
  be verified in the current snapshot before cleanup or retirement.
- Add a provider-free publish preflight that validates approved cover hashes and
  JPEG payload limits while estimating the request budget.

## [0.0.9] - 2026-08-24

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
- Add the original Drift Atlas v1 cover set for all 14 approved canonical
  playlists, including a darker user-selected Open-Sky Anthems revision and a
  local contact-sheet preview.
- Add strict artwork-manifest validation, content hashes, immutable Neon review
  batches, and explicit `artwork import|status|list|approve` commands without
  requesting Spotify image-upload scope or performing provider writes.
- Add immutable v0.0.9 apply-readiness assessments for snapshot freshness,
  approvals, operation ordering, destructive gates, interruption recovery,
  bounded Spotify 429 retries, idempotent replay, and explicit read-only OAuth
  scope probes through `sync readiness|readiness-show`.
- Define External Playlist Bookmarks for followed and externally collaborative
  Spotify/Apple Music playlists so their metadata and accessible contents can
  be retained in Neon before separately approved provider-library cleanup.
- Persist Spotify external bookmarks and immutable observations in Neon,
  separate them from the active library, detect provider snapshot changes,
  reuse readable collaborative contents, and add read-only bookmark list and
  track inspection commands.
- Add explicit one-bookmark refresh with immutable complete, inaccessible, and
  not-found attempt history; retain readable ordered tracks when Spotify
  permits access without adding requests to normal sync.
- Add immutable all-present-bookmark cleanup review batches, explicit approval,
  exact candidate inspection, and separately counted relationship-only cleanup
  operations in Spotify dry-run planner v5; provider writes remain disabled.
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

[Unreleased]: https://github.com/orbyts/chordrift/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/orbyts/chordrift/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/orbyts/chordrift/compare/v0.0.9...v0.1.0
[0.0.9]: https://github.com/orbyts/chordrift/compare/v0.0.5...v0.0.9
[0.0.5]: https://github.com/orbyts/chordrift/compare/v0.0.4...v0.0.5
[0.0.4]: https://github.com/orbyts/chordrift/compare/v0.0.3...v0.0.4
[0.0.3]: https://github.com/orbyts/chordrift/compare/v0.0.2...v0.0.3
[0.0.2]: https://github.com/orbyts/chordrift/compare/v0.0.1...v0.0.2
[0.0.1]: https://github.com/orbyts/chordrift/compare/v0.0.0...v0.0.1
[0.0.0]: https://github.com/orbyts/chordrift/releases/tag/v0.0.0
