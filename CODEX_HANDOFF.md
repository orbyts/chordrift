# Codex handoff

Read this file at the start of a new Chordrift task. It records durable product
decisions, current operational state, and the safest next action without
requiring the previous conversation. Update it whenever a task changes those
facts. Never add credentials, tokens, database URLs, private keys, or personal
archive contents.

Last updated: 2026-08-24.

## Project and repositories

- Project: Chordrift, a personal music-library intelligence and synchronization
  CLI written in Rust.
- GitHub: <https://github.com/orbyts/chordrift>
- Primary Codex workspace: `/Users/suhail/Documents/ChatGPT/Music`
- User's normal clone: `$CRATES/chordrift`, currently
  `/Users/suhail/Library/CloudStorage/Dropbox/matrix/crates/chordrift`
- Local Storexa clone, if its source is needed: `$CRATES/storexa`
- Released CLI: `chordrift 0.0.5`
- `main` is the Spotify-focused release line. The Apple foundation is isolated
  on `codex/apple-music` and must not be merged until it can be tested with real
  Apple credentials.

Before editing, inspect `git status --short`, the current branch, this file,
`ROADMAP.md`, and `docs/HOW_TO_CHORDRIFT.md`. Preserve unrelated user changes.

## Durable architecture decisions

- Neon PostgreSQL is the canonical and operational source of truth.
- Provider APIs are adapters. Current Spotify state is pulled into immutable
  snapshots; provider state does not compete with Neon authority.
- Chordrift is read-only against Spotify through the current v0.0.8 work. It
  does not create, modify, reorder, or delete remote playlists or tracks.
- Spotify downloadable archives are optional enrichment and immutable local
  recovery inputs. They are not the operational database.
- A routine `chordrift sync pull` reads live Spotify state and reconciles
  history already stored in Neon. It never scans local ZIP files.
- `chordrift history ingest` is the explicit command for newly downloaded ZIPs.
  It deduplicates cumulative exports at archive and event level, then archives
  the original ZIP locally.
- `chordrift history restore` replays retained archives for database recovery.
- Secrets belong in macOS Passwords/Keychain. Environment exposure is managed
  by Apogee. Do not modify `.zshrc`, `.bashrc`, or other shell initialization
  files.
- Minimize provider requests. Reuse Spotify playlist snapshots and saved-track
  baselines from Neon whenever their remote signatures are unchanged.
- The normal Neon/CLI playlist surface contains only playlists present in the
  latest successful Spotify snapshot and uses that snapshot's current names.
  Older names and removed playlists remain only in immutable sync/audit history;
  proposed Chordrift names remain separate until published.

## Product intent

Chordrift will become the canonical playlist orchestrator while discovery stays
native to each streaming platform. A small number of provider playlists can be
marked as discovery inboxes. Chordrift will consume new tracks, recognize
existing canonical tracks, and eventually clear or retire inbox/legacy
playlists only after every track is represented in an approved replacement.

Clustering and LLM-proposed playlist names must remain inspectable and require
explicit user approval. No track or playlist deletion may be implicit. Managed
playlist application must be idempotent, auditable, interruption-safe, and
converge to zero changes on repeated runs.

## Current Spotify implementation

Completed releases:

- v0.0.1: Storexa/Neon project skeleton and migrations.
- v0.0.2: Spotify PKCE authorization and read-only inventory.
- v0.0.3: canonical analysis, incremental pull, removals, playlist roles, and
  drift policies.
- v0.0.4: cumulative Spotify history ingestion/recovery and read-only query
  commands.

Useful commands:

```console
chordrift sync pull
chordrift playlists list
chordrift playlists tracks --name "Playlist name"
chordrift analyze summary
chordrift analyze overlap --limit 25
chordrift analyze duplicates --limit 25
chordrift history summary
chordrift history top --limit 25
```

The last verified history state contained 2 archives, 149,195 music events,
15,553 unique Spotify track IDs, 6,602.55 listening hours, and dates from
2014-11-05 through 2026-08-20. Treat these as a checkpoint, not hard-coded
expectations; query Neon for current values.

Local Spotify recovery inputs are Git-ignored beneath:

```text
data/spotify/personal/
├── inbox/
│   ├── account-data/my_spotify_data.zip
│   └── extended-streaming-history/my_spotify_data.zip
└── archive/<kind>/<date>/<sha256>/my_spotify_data.zip
```

Keep Spotify's original ZIP filename. Folder kind, date, and SHA-256 prevent
collisions. Do not commit anything beneath `data/`.

## Deferred Apple Music provider

The user plans to enroll in the Apple Developer Program eventually, partly for
another Swift project named Photara, but intentionally deferred the annual fee.
Apple Music must not block Chordrift's Spotify milestones.

The `codex/apple-music` branch contains an offline-tested foundation for:

- ES256 Apple developer-token signing;
- Keychain-backed Media Services and per-user credentials;
- loopback MusicKit user authorization;
- read-only catalog access;
- batches of up to 25 ISRC lookups;
- metadata fallback searches; and
- extended `audioVariants` detection, where `dolby-atmos` indicates Spatial
  Audio availability.

It has no user-facing CLI or persisted matching decisions and has not been
tested against Apple. When resuming, first create a MusicKit-enabled Media ID
and `.p8` Media Services key, then rebase the branch onto current `main` and
integration-test before designing migrations or merging.

Temporary Spatial Audio workaround:

1. Create a dedicated public Spotify playlist of candidates.
2. Scan it at <https://helloatmos.app/spotify/>.
3. Export the Atmos subset to a specially named Apple Music Spatial Audio
   playlist, or make a temporary filtered Spotify playlist and mirror it with
   SongShift.

Hello Atmos is a third party. Its matches are temporary convenience results,
not verified Neon provider state. Native Chordrift matching must retain exact
recording, storefront, timestamp, and evidence provenance.

Apple privacy exports do not require developer membership. Do not implement a
history parser from assumed examples; inspect the user's actual archive first,
then apply the same immutable, cumulative, PII-excluding principles used for
Spotify.

Neon remains the durable identity, provenance, history, and orchestration
ledger, while Spotify is the only active live provider for now. SongShift can
mirror every canonical Spotify playlist individually, so do not replace the
obsolete `Two Way Sync` playlist. Bootstrap the old Apple library once from the
two SongShift JSON exports rather than transferring it through temporary
Spotify playlists. SongShift remains a temporary publishing workaround, not an
authoritative native provider adapter.

The two 2026-08-24 exports are preserved under the Git-ignored
`data/apple/personal/bootstrap/` content-addressed archive. They contain 952
and 309 entries, 73 exact overlaps, and 1,185 unique Apple service IDs. Only 173
unique ISRCs are present, so do not create canonical tracks from loose fuzzy
matches. Automatically link unambiguous identities and stage ambiguous or
unmatched metadata for review.

Future native platforms are authoritative evidence for live user actions on
their respective surfaces. A deletion should create a provider-scoped
tombstone/override in Neon, not hard-delete the canonical track, history, or
provenance. Reconciliation must distinguish intentional user removal from
provider drift and prevent delete/re-add loops before propagating an action to
another platform.

The user approved the name **Excluded Tracks** for the provider-neutral view of
intentional removals. Only removal from a Chordrift-managed playlist after its
published state has been verified creates the account-level exclusion. Preserve
provider, time, prior canonical assignment, and restore history. Removals from
provider-curated, intake, transport, and legacy playlists are drift, not global
exclusions. Do not create a Spotify playlist for this internal view.

## Current roadmap and next task

Apple was removed from the critical release path. v0.0.5 is active on
`codex/embeddings`. The target representation is hybrid: a reusable pretrained
music-audio foundation vector plus an independently versioned account-specific
component. MERT v1 95M is the preferred first acoustic candidate; evaluate MuQ
as an alternative. Both published weight sets are currently non-commercial, so
revisit licensing before any commercial Chordrift use.

The acoustic models require waveforms. Do not download, scrape, or record
Spotify audio. Populate canonical acoustic embeddings only from lawful,
locally owned DRM-free audio. Spotify-only tracks receive a deterministic
semantic fallback from explicitly semantic playlist co-occurrence, artists,
albums, and historical playlist-name tokens. Keep listening behavior separate:
plays, recency, completion, skips, `On Repeat`, inbox state, and recommendation
provenance are preference/lifecycle signals for composition and ordering, not
musical-similarity dimensions.

Language and region are desired semantic dimensions, but Spotify does not
provide authoritative track language or origin. Do not equate availability
markets with origin and do not guess from titles. Plan provenance-aware
MusicBrainz enrichment for recording/release language, release country, and
artist area, retaining unknown values and confidence. Re-check Spotify's
current Platform policy before clustering ships. The intended operation is not
model training: independently resolve artist/title/ISRC, run a pretrained model
or import external semantic tags, and cache the inference with provenance,
model/version, confidence, and retrieval time. Spotify remains the sync and
user-action adapter.

Playlist policy has three distinct classes: provider-curated signal sources
(`On Repeat`, Daily Mix, prompted playlists), user-owned intake surfaces
(exact names: `Inbox`, `From Friends`, and `Liked from Radio`), and
Chordrift-managed canonical playlists. `Inbox` means a direct strong personal
discovery; `From Friends` means an explicit recommendation; `Liked from Radio`
means radio/autoplay discovery. Canonical outputs use approved generated vibe
names and are never intake. The temporary Atmos companion is `Chordrift Spatial
Audio`.
Never clear provider-curated sources. Clear intake entries only after Neon
retains provenance and a published canonical Spotify destination is verified.
Do not feed Chordrift-managed output back into semantic training; use previous
assignments only as stability constraints.

The intended final Spotify surface contains the three intake playlists,
Spotify-managed sources, multiple Chordrift-managed canonical playlists with
approved generated names, and the temporary `Chordrift Spatial Audio`
companion. All other user-created legacy vibe and utility playlists are to be
retired only after their semantic evidence is consumed and every track has a
published, verified canonical destination. The user explicitly added
`Melodi(es)` and `Ambient Music Therapy – Indian Lounge - Relaxing Music for
your Six Senses` to that retirement set; both currently remain
`semantic_legacy` with weight 1.0. Retirement removes playlist containers, not
tracks. Spotify Liked Songs remains a provider library surface.

The user also wants followed/shared playlists owned by other people removed
from the visible Spotify and future Apple Music library surfaces. Treat these
as provider-neutral **External Playlist Bookmarks**, including externally owned
collaborative playlists. Before cleanup, retain provider ID, owner, link,
relationship, metadata, and an immutable last-known content snapshot when
accessible; explicitly mark inaccessible content. Bookmarks contribute no
semantic or behavioral signal, do not count as active canonical library
playlists, and are never legacy-retirement sources. A separately approved
cleanup removes only the user's provider-library relationship, never edits or
deletes the source owner's playlist. Neon keeps the bookmark for later
inspection. v0.0.9 must add read-only bookmark list/content inspection and
cleanup planning. The current Spotify importer only reports a skipped-followed
count, so it does not yet satisfy this bookmark requirement.

Migrations 0009-0011 and the CLI keep canonical `track_embeddings`
separate from immutable account-scoped `embedding_generations` and
`account_track_embeddings`. New commands cover input audit, playlist semantic
weights, deterministic generation/status, and nearest-neighbor inspection.
The live Neon database is current at 11/11. `Collaboration Jessica ` is ignored;
`Liked from Radio` is discovery intake. Signal generation v2
`4fa57f0d-fce1-4c95-8d85-bba9d206afe2` covers 2,005 tracks: 1,554 history,
927 saved, 30 rotation, 102 discovery, 65 intake, and zero recommendation or
prompted tracks. Semantic audit finds 666 playlist-connected, 1,469
artist-related, and 1,015 album-related tracks.

The 128-dimensional diagnostic generation exposed obvious hash collisions. A
1,024-dimensional generation (`a33ef4ef-bd70-4375-9cc5-ca2f2ef59eb7`) embeds
1,733 of 2,005 tracks and produced materially cleaner inspected neighbors:
Nine Inch Nails remained with Nine Inch Nails/Trent Reznor, and the spurious
A. R. Rahman collision disappeared. The code default is now 1,024. Treat this
as an inspectable semantic fallback, not the final acoustic representation or
authorization to publish/modify playlists.

## Verification and release discipline

Before committing a code change, run checks proportional to the change. The
normal full baseline is:

```console
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo test --doc
cargo package
```

Postgres integration tests require the disposable test database environment
documented in the repository. Apply migrations to live Neon only after local
and disposable-database verification. Keep `docs/HOW_TO_CHORDRIFT.md` updated
for every CLI leaf command; `tests/user_docs.rs` enforces that coverage.

For a release: confirm CI is green, tag the exact tested commit, create the
GitHub release, publish to crates.io, install the released version, fast-forward
`$CRATES/chordrift`, and verify the installed CLI against Neon. Never expose
credentials in command output, commits, release notes, or this handoff.

## Handoff maintenance

At the end of each focused task:

1. Update the date and any changed decisions, versions, branches, migrations,
   operational checkpoints, and next action.
2. Remove stale claims rather than appending a chronological transcript.
3. Link detailed permanent documentation instead of duplicating it here.
4. Confirm the handoff contains no secrets or unnecessary personal data.
5. Leave the active branch and working tree state explicit for the next task.

v0.0.5 was merged at `ffbcc40`, tagged, released on GitHub, published to
crates.io, installed locally, verified against healthy Neon, and fast-forwarded
into `$CRATES/chordrift`. Active development is on
`codex/semantic-enrichment` for v0.0.6; PR #2 is open. Migration 0012 is live
and CI run `32765541591` passed. The bounded MusicBrainz adapter caches raw ISRC
and recording-detail responses separately, respects the one-request-per-second
limit, and persists conservative match/fact provenance. A live high-priority
probe matched M83 and retained 35 useful genre/tag/release facts. Pending
requests are prioritized by intake, rotation, saved state, then meaningful
plays; these values affect scheduling only, never similarity. The separate
cache-first `enrich artists` operation retains MusicBrainz
primary-associated-area evidence without claiming birthplace, nationality, or
track language. CI run `32767604600` passed, including migration 0013 on
disposable PostgreSQL 18; migration 0013 is live and Neon is healthy at 13/13.
A three-artist probe considered M83, Reinoud Ford, and Keaton Henson: two
primary areas resolved, one transient request was cached for later retry, and
two track-level facts were written. An immediate repeat converged with zero
artists, requests, or writes. Current live coverage is 2 matched tracks, 39
MusicBrainz facts, and 2 tracks with artist-area facts. Pretrained mood/sound
inference is next; Excluded Tracks remains future work.

The pretrained-audio boundary is model-neutral and requires authorized local
audio; Chordrift will not download or infer from Spotify audio. Migration 0014,
`enrich model-import`, `enrich model-status`, and the strict path-free
`docs/model-inference-v1.schema.json` are implemented. Artifacts pin model
name/version/revision/license, input hashes, inference time, aggregation,
embeddings, and mood/sound facts. MERT and MuQ-MuLan are candidate foundation
spaces and Essentia provides explicit classifiers, but the evaluated weights
carry non-commercial terms and all require real audio. Tracks without lawful
audio remain unembedded rather than receiving invented acoustic evidence. CI
run `32769042155` passed; migration 0014 is live and Neon is healthy at 14/14.
The live model status correctly reports 2,005 eligible tracks and zero imported
inferences, embeddings, facts, or models because no authorized local audio has
been supplied.

Manual correction is explicitly post-generation: after proposed playlists
exist, the user must be able to reject a track's current vibe and optionally
choose or lock another destination. The next generation moves it and retains
that account-scoped decision as an auditable stability constraint while
preserving the original model score and assignment history. Do not implement
this as a free-floating pre-clustering mood tag; wait for stable cluster and
playlist identities.

The semantic fallback model is now `semantic-feature-hash@3`; its immutable
input includes MusicBrainz facts, imported model facts, and
deterministically projected lawful acoustic vectors in addition to legacy
playlist, artist, album, and historical-name evidence. Source/parser and
model/version identities are recorded in generation parameters; behavior
remains excluded. Migration 0015 and the `clusters generate/status/list/tracks`
commands are implemented using deterministic spherical k-means, an exact
embedding-generation input, explicit low-similarity/undersized unassignment,
idempotent generation hashes, and temporary content-derived machine labels.
Cluster output is diagnostic and cannot create or modify Spotify playlists.

CI runs `32770150613` and `32770941996` passed. Migration 0015 is live and Neon
is healthy at 15/15. Live embedding generation
`f0c8eda3-ad34-41b9-a362-2fb56354bb95` is model v3, 1,024 dimensions, and covers
1,733 tracks. The first all-track centroid fit exposed bad 2–3-track groups and
a 650-track catch-all, so it was superseded by semantic-seeded algorithm v2.
The current diagnostic generation `8ec8512f-66fc-4f59-a50e-65d5b7ac8d13`
contains 12 clusters of 30–251 tracks and leaves 895 weakly supported tracks
unassigned. An identical command reused the generation. Samples show coherent
M83 and A. R. Rahman groups, while a generic legacy-playlist cluster still has
many equal scores; do not mistake this sparse-evidence fallback for final
acoustic classification or publish its machine labels. More independent
semantic/acoustic coverage is still needed before these clusters should be
published.

v0.0.7 proposal work is implemented on `codex/semantic-enrichment`. Migrations
0016 through 0018 are live and Neon is healthy at 18/18. Migration 0017 provides
the latest-snapshot-only Spotify playlist surface. The `proposals` commands
provide a non-destructive workflow with stable `playlist-*` concepts, overlap-based
lineage, strict naming artifacts, complete generator/hash provenance, and
explicit approval. The first live proposal is
`ca81d1b2-e56b-41e6-8846-cdb379cb039b`, derived from cluster generation
`8ec8512f-66fc-4f59-a50e-65d5b7ac8d13`. It contains 12 playlists and 838
assigned tracks. All 12 candidate names were imported as an OpenAI Codex GPT-5
naming revision. Two additional manual categories, `Open-Sky Anthems` and
`Weightless Horizons`, were created and 46 initially uncovered tracks were
reviewed into stable destinations. The proposal now contains 14 named playlists
and represents all 699 of 699 legacy/intake tracks; `proposals missing` is
empty. The account owner explicitly approved generation
`ca81d1b2-e56b-41e6-8846-cdb379cb039b`; Neon reports `approved`, 14/14 named,
and complete coverage. No Spotify state was changed.
Migration 0018 and `proposals category-create/assign/review` add
stable manual semantic destinations, reversible active decisions, complete
revision history, a non-provider needs-review state, and replay into later
proposal generations.

v0.0.8 adds migration 0019 and `sync plan` / `sync plan-show`. A plan is an
immutable Neon audit record bound to an exact approved proposal and exact
imported Spotify snapshot. Identical inputs reuse the same plan. Operations are
ordered into publish, reconcile, cleanup, and retirement phases. Inbox cleanup
and legacy retirement are deferred behind publication/verification gates, and
retirement additionally requires separate future approval. The planner makes
no Spotify request and Spotify write scopes remain disabled. Migration 0019
also introduces stable concept mappings for future published provider
playlists and the provider-neutral reversible `excluded_tracks` ledger.
Migrations 0019 and 0020 are live and Neon is healthy at 20/20. Migration 0020
adds immutable successful managed-playlist baselines so a later missing
expected track becomes an internal `exclude_track` proposal rather than an
automatic re-add; an unexpected extra remains ordinary provider drift. The
current planner is `spotify-dry-run-v4`; earlier development plans remain
immutable audit artifacts and must not be applied. The current verified v4 plan is
`cda2639d-da67-4b23-9492-a9274c71088c`, bound to approved proposal
`ca81d1b2-e56b-41e6-8846-cdb379cb039b` and Spotify snapshot
`622a94b4-b60e-4f26-8da2-20e540e160c1`. It contains 1,007 exact operations:
16 creates (14 canonical plus missing `Inbox` and `From Friends`), 884 ordered
track additions, 65 deferred intake removals, and 42 separately approved
legacy retirements; no renames, restorations, or exclusions before initial
publication. `Liked from Radio` already exists and is reused. Every inspected
retirement has complete preservation.
The snapshot is current and identical v4 planning reuses this plan ID. Next is
v0.0.9 apply-readiness validation; it must remain read-only against Spotify.
That milestone also generates one simple original deterministic cover per
canonical playlist from its approved name/description/tags, stores generator
version and SHA-256, produces a contact-sheet-style preview, and requires
explicit artwork approval. Do not request Spotify image-upload scope or upload
covers until v0.1.0.

Apogee configures a machine-wide shared Cargo target. Because the released
`$CRATES/chordrift` clone and this development workspace currently share the
same package name/version, a plain `cargo run` reused an older final executable
that lacked `proposals`. For unreleased development commands, use a repository-
specific target such as `cargo run --target-dir target -- ...`; do not modify
shell initialization files.
