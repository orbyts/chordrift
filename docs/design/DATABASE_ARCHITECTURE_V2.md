# Chordrift database architecture v2

Status: approved direction, 2026-08-26. This document defines the storage
redesign that precedes recipe work and the native UI. It is not authorization
to delete the current Neon database.

## Why this redesign exists

The v0.1.2 personal database is healthy and fully migrated, but its physical
representation is unnecessarily expensive. A read-only audit measured about
391 MB of PostgreSQL data while Neon reported about 0.43 GB of project storage.
The project has one branch and a six-hour restore window, so extra branches are
not the cause.

The dominant costs are:

- `listening_events`: about 275 MB for 149,314 events. Every archive event
  repeats track, artist, album, source-file, and raw playback JSON. Eight
  indexes consume about 90 MB, including two large event-identity indexes.
- `provider_playlist_tracks`: about 32 MB for 103,561 rows. Fifty-eight full
  provider snapshots were recorded in eight days even though the current
  library contains only 1,790 playlist entries.
- derived statistics, embeddings, sync operations, and verified-history tables
  account for most of the remainder.

The reported 13,855 unmatched historical tracks are not current Spotify
library members. They are provider track identities observed across twelve
years of listening archives for which current Spotify metadata has not yet
been resolved. Preserve them as lightweight historical identities and enrich
them lazily; do not treat them as current-library bloat.

## Data classes

Chordrift must give every table one explicit lifecycle class.

### Current provider state

Store one transactionally replaceable, verified view per account and provider:
accounts, playlists, current membership and order, saved surfaces, provider
identities, provider revisions, and capture time. A routine pull updates this
state or reuses it by provider revision. It does not copy a complete membership
body merely to prove that nothing changed.

### Durable Chordrift intent

Provider-neutral user intent is authoritative and retained: canonical
collections, intake surfaces, the Re-evaluate queue, classifications,
corrections, exclusions, retirements, recipe definitions, approvals, and
verified apply receipts. Provider adapters translate this intent but do not
own it.

### Listening evidence

Retain normalized event facts: account, provider identity, playback time,
duration, completion/skip evidence, context when available, and source import.
Store track/artist/album display metadata once per historical provider identity,
not once per event. The immutable Spotify ZIP and its checksum are the raw
source of truth; PostgreSQL retains its manifest, coverage, import result, and
normalized facts rather than duplicating every raw JSON object.

### Rebuildable derived data

Statistics, analyses, embeddings, similarity results, recipe candidates, and
recommendations are caches. Each generation records its algorithm/schema
version and inputs. Old generations may expire after no durable plan or audit
references them.

## Modeling rules

- Keep canonical musical identity separate from provider identity and account
  membership.
- Keep current state separate from immutable evidence and user intent.
- Use typed columns for fields used by recipes, filters, constraints, or
  indexes. Reserve JSONB for genuinely provider-specific extensions.
- Make provenance content-addressed. A source archive/file is stored once and
  referenced by events.
- Keep unmatched historical identities small and resolvable on demand.
- Store full checkpoints only when they protect a bounded operation or named
  release. Represent routine provider evolution as current state plus compact
  changes and verified receipts.
- Every deletion or compaction command supports plan, apply, and verify phases;
  it must be resumable and report preserved invariants.
- Do not partition 149,314 events merely for fashion. Introduce time
  partitioning only when measured query or maintenance behavior warrants it.

## Retention policy

- Current provider inventory: one materialized current state per account.
- Pre-apply provider checkpoints: retain temporarily, initially 30 days.
- Named release/baseline checkpoints: retain selectively in compact form.
- Verified plans, approvals, and apply receipts: retain permanently without
  embedding complete provider inventories.
- Normalized listening events: retain permanently.
- Raw provider archives: retain permanently outside PostgreSQL with hashes.
- Per-event raw archive JSON: remove only after archive verification and a
  successful restore/rebuild rehearsal.
- Derived caches and unreferenced generations: expire automatically.

## Approved migration strategy

Do not mutate or discard the v0.1.2 database in place as the first step.

1. Preserve a custom-format logical dump, schema, restore catalog, and checksum
   outside Neon.
2. Restore it into an isolated rehearsal database and prove that it is usable.
3. Implement v2 schema and compaction as versioned, tested migrations and
   explicit CLI plan/apply/verify commands.
4. Materialize the current Spotify state as the new provider baseline.
5. Migrate durable Chordrift intent and normalized listening evidence.
6. Rebuild statistics and embeddings instead of copying stale cache rows.
7. Compare invariants before cutover: provider surface counts and order,
   canonical assignments, exclusions, queue state, 149,314 listening events,
   listening duration, first/last playback, import hashes, and verified apply
   history.
8. Cut over configuration only after both databases pass comparison.
9. Keep the old dump through an observation period; delete the old Neon data
   only with a separate exact approval.

The initial physical target is approximately 100–150 MB without sacrificing
meaningful history. This is a target to verify, not a guaranteed result.

## Workstream boundaries

The work should be split into sequential Codex tasks, not concurrent branches
editing the same schema:

1. **Safe cleanup foundation:** restore rehearsal, invariant report, storage
   report, and provider-free compaction plan. No production deletion.
2. **Database v2:** schema, retention model, migrations, tests, and compact
   current-state persistence.
3. **Migration and cutover:** rehearse, migrate, compare, switch, observe, and
   eventually retire old storage with approval.
4. **Code refactor:** move provider queries, recipes, caches, and diagnostics to
   v2 APIs; remove compatibility code only after cutover.
5. **Native UI:** build on stable v2 query/command DTOs and background-operation
   contracts rather than coupling UI code to legacy tables.

Tasks 1–4 are causally dependent. UI design may proceed separately, but UI
implementation should not bind to the database until the v2 bridge contracts
are stable.

## Existing backup baseline

The pre-compaction backup is stored at:

`$DROPBOX/Music/Chordrift/Backups/2026-08-26-pre-compaction/`

It contains the custom-format dump, schema-only SQL, `pg_restore` catalog, and
SHA-256 checksum. The catalog has been parsed successfully. A complete restore
rehearsal remains the first task.
