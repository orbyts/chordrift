# Codex handoff

Read this file at the start of a new Chordrift task. It records durable product
decisions, current operational state, and the safest next action without
requiring the previous conversation. Update it whenever a task changes those
facts. Never add credentials, tokens, database URLs, private keys, or personal
archive contents.

Last updated: 2026-08-26.

## Start the next task here: approve or defer production database-v2 data migration

The first exact-confirmed production data-migration attempt using plan
`a850fb15603f82c934daa127cfb768084938bc8ac601b6f30643ebc3a84e2ae8`
was rejected by PostgreSQL with SQLSTATE `53100` (insufficient storage). Do not
retry blindly. The transaction rolled back logically: normalized events,
historical identities, evidence imports, checkpoints, and migration receipts
all remain zero; all 464 durable references still await checkpoints; the plan
hash is unchanged and applicable; and the full legacy invariant is unchanged.

The aborted inserts nevertheless allocated dead physical pages. Production now
reports 514,457,600 database bytes and 503,087,104 ordinary-table bytes;
`normalized_listening_events` has zero visible rows but 98,500,608 total bytes,
and `historical_provider_track_identities` has zero visible rows but 4,128,768
total bytes. Production remains healthy at 43/43 migrations. No vacuum,
compaction, quota change, retry, read cutover, deletion, connection change, or
Spotify operation was performed.

The safest next step is to add sufficient Neon storage headroom before another
exact-plan attempt. A targeted vacuum/reuse strategy is a separate production
maintenance decision and must not be inferred from the existing data-migration
approval. Re-run the read-only status, invariant, storage, and exact plan after
the storage decision, then request a fresh bounded retry approval.

The next conversation begins from the pushed
`codex/database-v2-migration-rehearsal` branch. Do not resume the completed South Asian,
legacy-route, Inbox, or Liked Songs cleanup. First read `README.md`, the
v0.2.0-and-later sections of `ROADMAP.md`, this section, and
`docs/HOW_TO_CHORDRIFT.md`.

The safe cleanup foundation, additive database-v2 schema, complete local
migration rehearsal, read-only production preflight, and additive production
schema gate are complete through `codex/database-v2-migration-rehearsal`.
Production is now 43/43 migrations with exact legacy/current-state parity. The
next decision is whether to authorize the exact-confirmed normalized-evidence
and checkpoint apply using the production-emitted plan hash recorded below.
Read cutover, observation-window start, cleanup, and connection changes remain
later gates. Until separately approved, do not run them, delete any legacy row,
write to Spotify, begin recipes, or implement the native UI.

The current v0.1.2 database is healthy but its production physical footprint
was about 391 MB because raw metadata is repeated across 149,314 listening
events and complete playlist membership is copied across routine snapshots. A
fresh logical restore occupies 249,657,023 bytes, proving that some production
storage is churn/bloat while substantial logical duplication remains. The
13,855 unmatched tracks are lightweight historical identities from the Spotify
archive, not current library members; preserve and resolve them lazily.

A verified pre-compaction backup exists at
`$DROPBOX/Music/Chordrift/Backups/2026-08-26-pre-compaction/`. It contains a
25 MB custom-format dump, schema SQL, parsed `pg_restore` catalog, and SHA-256
checksum. The dump hash
`8c5796cba5729931678f825021fe03268b81129352349266d7a68b487b3711ae`
was verified, then restored with exit-on-error into isolated local PostgreSQL
18.6. The restore has all 39 successful migrations and zero failed migrations;
`pg_amcheck` passed all 676 relations / 30,156 pages. Production was not
mutated, its connection was not changed, and Spotify was not contacted.

Reusable read-only commands are now:

```console
chordrift db invariant-report --account personal
chordrift db storage-report
chordrift db compact plan --account personal
chordrift db v2 status --account personal
chordrift db v2 migration plan --account personal
chordrift db v2 migration verify --account personal
chordrift db v2 cutover-plan --account personal
```

The compaction planner starts a read-only transaction and rolls it back. The
rehearsal contains 58 provider snapshots: keep one current, 41 older snapshots
are protected by durable plan/verification/generation/bookmark/intent history,
and 16 older routine snapshots have no such durable reference. Those 16 contain
26,490 repeated playlist membership rows plus repeated saved-surface rows.
This is measured planning evidence, not authorization to delete anything.

The invariant baseline is 22 playlists / 1,790 exact ordered memberships /
1,765 unique playlist tracks; 16 canonical playlists / 1,754 unique canonical
assignments; 107 active exclusions; one empty active Re-evaluate surface; zero
saved tracks or albums; 149,314 active listening events across 15,575
historical identities (1,720 matched / 13,855 unmatched); 23,769,184,794 ms
from 2014-11-05 through 2026-08-26; two archive import hashes; and 19 verified
apply runs. Exact order/hash values and full physical sizes are recorded in the
database-v2 design. The earlier final zero-operation sync plan remains
`56a0d535-f83e-42ae-898e-8ed627e6f4e9`; the newest stored plan
`fac3d2ba-6b6e-47e6-9575-24e10fa4458b` contains one reconcile
`exclude_track`, which must remain visible as a pending intent delta during v2
comparison.

Migration `0040_database_v2_foundation.sql` is additive. It adds one current
provider inventory per account, content-addressed playlist/saved-surface
revisions, compact checkpoint tables, historical provider identities, typed
normalized listening evidence, and nullable checkpoint references for plans
and managed verifications. The Spotify importer dual-writes the compact current
state while retaining legacy snapshots until cutover. Repeating an identical
fixture creates no duplicate v2 playlist or saved-surface revision.

The migration was applied only to a clone of the restored PostgreSQL 18.6
database. It completed in 153 ms with 40/40 migrations successful. The complete
v1 invariant report was byte-identical before and after; `pg_amcheck` passed
758 relations / 30,301 pages. The v2 current state
has one inventory, 22 current playlist pointers, 22 immutable revisions, and
1,790 ordered revision tracks; current playlist order and both saved surfaces
match exactly. Database size increased only from 249,657,023 to 251,205,311
bytes for the additive schema/backfill.

Migrations 0041 and 0042 add the exact-confirmed migration/receipt surface and
local listening-evidence dual-write compatibility. Migration 0043 makes report
and current-inventory hashes stable across Neon `C.UTF-8` and local
`en_US.UTF-8` collations. The new local rehearsal clone reached 43/43
migrations. Exact plan
`a850fb15603f82c934daa127cfb768084938bc8ac601b6f30643ebc3a84e2ae8`
migrated all 149,314 events and 15,575 identities, both archive manifests, 43
plans, 420 managed verifications, and one cleanup approval. Forty-one referenced
snapshots deduplicated to 24 compact checkpoints. A second apply with the same
hash was idempotent.

Independent verification matches 23,769,184,794 ms, first/last timestamps,
100,926 matched events, 1,720 matched identities, and 13,855 unmatched
identities exactly. The original invariant report is byte-identical before and
after. `db v2 status` now reports `ready_for_cutover: true` on this rehearsal,
and `pg_amcheck --parent-check --heapallindexed` passes. The fresh dual-storage
rehearsal is 358,815,423 bytes; normalized events plus identity metadata are
smaller than the legacy event relation, but no legacy deletion is approved.

Individual archive-member hashes were not stored by v1. The migration records
17 known event-bearing paths as `archive_manifest_only` with null member hashes
instead of fabricating digests; the verified containing ZIP hashes remain
authoritative. Migration 0042 dual-writes future local archive/recent-event
changes into normalized evidence during the rollback observation window.

The read-only rehearsal cutover plan hash is
`32f1e7f3e9899c72a822a5faf588c29dc905d62ead3b3b17313d165d6e4640b8`.
Never apply that hash blindly to production.

The read-only production preflight and additive schema gate are complete.
Backup hash
`8c5796cba5729931678f825021fe03268b81129352349266d7a68b487b3711ae`
still verifies; Neon is healthy on PostgreSQL 18.6 with 43/43 migrations; and
production/base invariant reports are byte-identical with stable playlist
fingerprint
`d3186b303fa7d7dabe4d45f605d8a0d97a132fe50cd2bc00368491570f83e90b`.
All 17 non-empty playlist hashes match individually. A read-only prospective
production current-state calculation matches the rehearsal hash
`f12ef35e6ac961c99819be5d667eb60273435c25f0dd5b6f9182b369ba8e0ff3`.
After the additive schema gate, production measures 411,852,800 database bytes
and 400,482,304 ordinary-table bytes. The compaction classification remains
one current, 41 protected, and 16 redundant snapshots. No production write or
Spotify request occurred.

Production emitted migration data-plan hash
`a850fb15603f82c934daa127cfb768084938bc8ac601b6f30643ebc3a84e2ae8`,
with `applicable: true`: 149,314 events, 15,575 identities, two archive
manifests, 41 checkpoint source snapshots, 43 plan references, 420 verification
references, and one cleanup reference. Current v2 state is exactly 22 playlists
and 1,790 ordered memberships; normalized evidence and checkpoints remain
empty, proving the data apply has not run. The next bounded gate is only
`chordrift db v2 migration apply --account personal --confirm <exact hash>`,
followed immediately by read-only invariant/status/verify/storage reports and a
stop. Read cutover, observation window, and legacy cleanup remain distinct
approval gates.

The approved direction is broader than static playlist organization. Chordrift
becomes a personal listening-system designer with three distinct layers:

1. **Canonical collections** retain durable identity, membership rationale,
   user-approved boundaries, and artwork.
2. **Intake surfaces** retain discovery/review provenance. Like is the least-
   friction intake; named inboxes add context; `Re-evaluate` means keep but
   reconsider the current destination.
3. **Generated listening playlists** are renewable, versioned recipe outputs.
   A track may appear in several outputs without duplicating its canonical
   identity.

Recipes support both curated automatic presets and advanced controls. Initial
dimensions are discovery recency, recent rotation, old favorites, forgotten
favorites, recommendations, collection eligibility and diversity,
new-versus-familiar balance, artist spacing, target duration, repetition and
cross-output budgets, ordering philosophy, and user-defined sections. Every
generation needs immutable inputs, recipe version, evidence capabilities,
constraints, random seed, per-track reasons, and final order.

Data availability is first-class. Provider adapters report capabilities;
recipes declare required and optional evidence. Spotify save timestamps can
drive New Discoveries, Recently Played supplies bounded short-term evidence,
and the optional extended-history archive unlocks trustworthy lifetime repeats,
skips/completions, forgotten favorites, and deep rediscovery. Missing evidence
disables a feature or reduces confidence with a visible explanation.

User intent has strength: hard collection boundaries, strong preferences, soft
ranking facts, and one-generation choices are not interchangeable. Corrections
become account-scoped evidence. Chordrift may propose a broader reusable rule
after repeated consistent corrections, but cannot silently activate it. A
future classic-Hindi collection remains a v0.2+ review candidate; use
`Re-evaluate` until the evidence supports a coherent collection, name, and
artwork.

The intended onboarding is agentic but permission-bounded: read-only inventory
and audit; explain findings and missing capabilities; propose collections,
intakes, recipes, cleanup, names, and artwork; let the user edit or approve;
show an exact diff; obtain separate publication and destructive-cleanup
approval; execute through immutable readiness/apply gates; pull and prove
zero-operation convergence.

Recommended first slice: design the provider-neutral recipe and capability
model plus deterministic, provider-free preview for one `New Discoveries +
Rediscovery` recipe. Do not start with scheduling or a large UI. Reuse canonical
identity, classifications, complete inventory, listening signals, immutable
proposals, readiness, apply receipts, and verification.

## Operator and development requirements

- Normal product work runs quietly in the background. Do not design workflows
  that open Terminal windows, visible helper shells, or repeated browser
  pop-ups. OAuth may open the system browser only when consent is required and
  must return to a clear success/failure state in the app.
- The future signed native app owns scheduling, progress, cancellation,
  notifications, retry, and interruption recovery. Long work must never look
  stalled and must be safe to resume.
- Personal secrets are managed through 1Password and OS Passwords/Keychain.
  Apogee may load approved development environment variables, but Chordrift
  must not require Apogee for end users. Never print secret values or place
  them in config files, logs, shell history, source control, launch agents, or
  shell startup files. Use secret references or already-loaded variables.
- Keep user-editable non-secret configuration under
  `$XDG_CONFIG_HOME/chordrift`. A future preferences UI reads and writes the
  same schema so advanced users retain direct control.
- During Codex operations, inspect first, generate immutable/provider-free
  plans, report exact effects, request the smallest necessary approval, then
  apply and verify. Consolidate predictable permissions before long unattended
  work, but stop for artwork approval and final provider mutation approval.
- Do not launch GUI applications merely to show that a file exists. Give paths
  and instructions unless interactive OAuth, visual approval, or explicit user
  direction requires a launch.
- Prefer background-friendly, non-interactive commands and concise progress
  updates. Never expose tokens, database URLs, or 1Password values in command
  output.

## v0.1.2 release closure

- The live personal Spotify reconciliation is complete. Four approved
  destinations and their covers were published, 598 placements applied, 145
  consumed Inbox entries cleared, and Monsoon Cinema plus three legacy Route
  playlists retired.
- Liked Songs uses the account-scoped
  `clear-after-verified-assignment` policy. Of 346 supported saved tracks, 345
  had verified canonical placements and one had a durable exclusion. Cleanup
  apply run `39bc8d9b-1d05-4e13-aa5e-2d667ac4eaf0` removed all 346 with zero
  failures and was provider-verified against snapshot
  `7bca3d63-48d3-4193-afca-fc9cb634360c`.
- Final immutable plan `56a0d535-f83e-42ae-898e-8ed627e6f4e9` contains zero
  operations. The current Spotify surface has 22 playlists, 1,791 entries,
  1,766 unique playlist tracks, zero supported saved tracks, and no duplicate
  entries.
- The provider briefly reported one unsupported/unavailable saved item; it was
  never treated as an addressable supported track. The final stable pull
  reported zero unsupported items as well.
- A possible dedicated classic Hindi cinema destination is deferred to v0.2.0.
  Until reviewed as a coherent collection, move such edge cases into
  `Re-evaluate`, export a CSV if the queue grows, and classify them explicitly.

## Private classification sidecar (completed v0.1.2 foundation)

- Migrations `0036_user_track_classifications.sql` and
  `0037_classification_decisions.sql` add set/clear revision history and
  exact-approval CSV batches. User facts do not overwrite external/model facts.
- CLI: `classify set`, `clear`, `history`, `export`, `import`, and `approve`.
- CSV rows are inert unless `action` is `set` or `clear`; imports are drafts;
  approval must repeat the exact batch UUID.
- Embedding model v5 adds active collection/region/tradition/language facts in a
  `user-classification@v1` namespace at weight 1.25. Notes are explainability
  only. The base acoustic model vector is unchanged.
- The Monsoon Cinema and North/South review was completed in v0.1.2. Its
  resulting South Asian destinations were published and the legacy containers
  retired. Do not repeat that migration.
- Interactive `tracks inspect` now presents human sections and tables; raw
  provenance is opt-in with `--technical`. Interactive tables use the full
  detected terminal width. A later configuration layer belongs at
  `$XDG_CONFIG_HOME/chordrift/config.toml` and should cover width/layout, color,
  inspection detail, and date formatting.
- Migration `0038_user_classification_cohorts.sql` adds account-scoped,
  multi-valued personal cohorts. `user_cohorts` is backward-compatible in CSV
  schema v1 and is deliberately excluded from the sound embedding; it is future
  composition intent. The dedicated glossary and copy/paste templates are in
  `docs/how-to/CLASSIFICATION_DIMENSIONS.md`.
- The current schema is shaped for multiple accounts, and classification lookup
  now proves selected-account library membership. A complete two-account and
  provider-neutrality audit remains mandatory before a friend trial; scope is
  recorded in `docs/design/ACCOUNT_AND_PROVIDER_BOUNDARIES.md`.

## Project and repositories

- Project: Chordrift, a personal music-library intelligence and synchronization
  CLI written in Rust.
- GitHub: <https://github.com/orbyts/chordrift>
- Primary Codex workspace: `/Users/suhail/Documents/ChatGPT/Music`
- User's normal clone: `$CRATES/chordrift`, currently
  `/Users/suhail/Library/CloudStorage/Dropbox/matrix/crates/chordrift`
- Local Storexa clone, if its source is needed: `$CRATES/storexa`
- Current release: `chordrift 0.1.2`
- `main` is the Spotify-focused release line. The Apple foundation is isolated
  on `codex/apple-music` and must not be merged until it can be tested with real
  Apple credentials.

Before editing, inspect `git status --short`, the current branch, this file,
`ROADMAP.md`, and `docs/HOW_TO_CHORDRIFT.md`. Preserve unrelated user changes.

## Durable architecture decisions

- Neon PostgreSQL is the canonical and operational source of truth.
- Provider APIs are adapters. Current Spotify state is pulled into immutable
  snapshots; provider state does not compete with Neon authority.
- Chordrift can write to Spotify only through immutable inspected plans, exact
  readiness assessments, phase-scoped confirmation, resumable operation
  receipts, a post-write pull, and provider convergence verification. Preserve
  those gates for the native UI and every future provider adapter.
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

The concise product thesis is **a clean listening surface backed by lossless
musical memory**. The problem is accumulated playlist/library entropy: old
favorites, unexplained songs, followed or shared playlists, provider discovery,
and abandoned user lists become indistinguishable, so the user stops exploring
their own rich history and defaults to song radio. Chordrift must first explain
where each playlist and track came from, retain the best available provenance
and history, explicitly mark gaps, and only then simplify the provider surface.
The result should be a small set of purposeful, approved, artwork-backed
playlists the user genuinely listens to—not merely more generated playlists.

Chordrift will become the canonical playlist orchestrator while discovery stays
native to each streaming platform. A small number of provider playlists can be
marked as discovery inboxes. Chordrift will consume new tracks, recognize
existing canonical tracks, and eventually clear or retire inbox/legacy
playlists only after every track is represented in an approved replacement.

Clustering and LLM-proposed playlist names must remain inspectable and require
explicit user approval. No track or playlist deletion may be implicit. Managed
playlist application must be idempotent, auditable, interruption-safe, and
converge to zero changes on repeated runs.

If this personal workflow proves valuable, a future UI should expose the same
audited model: active library, external bookmarks, immutable history,
“why is this here?” provenance, bounded cleanup approvals, artwork review, and
manual vibe corrections. Do not assume a commercial multi-user product before
validating that the problem and workflow generalize.

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
(exact names: `Inbox`, `From Friends`, `Liked from Radio`, and `From Prompts`), and
Chordrift-managed canonical playlists. `Inbox` means a direct strong personal
discovery; `From Friends` means an explicit recommendation; `Liked from Radio`
means radio/autoplay discovery; `From Prompts` means a track intentionally
carried forward from a Spotify prompt-generated playlist. Canonical outputs use approved generated vibe
names and are never intake. The temporary Atmos companion is `Chordrift Spatial
Audio`.
Never clear provider-curated sources. Clear intake entries only after Neon
retains provenance and a published canonical Spotify destination is verified.
Do not feed Chordrift-managed output back into semantic training; use previous
assignments only as stability constraints.

The intended final Spotify surface contains the four intake playlists,
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
inspection. The first v0.0.9 slice adds `external_playlist_bookmarks`,
immutable pull-bound bookmark observations and track snapshots, plus
`chordrift bookmarks list|tracks`. The importer routes externally owned
collaborative and public followed playlists away from the active library,
copies unchanged readable collaborative contents from Neon, and retains
metadata-only public followed records under Development Mode. Private
Spotify-owned personalized surfaces remain active provider-curated signals.
Migration 0022 and the bookmark cleanup commands now provide immutable
all-present candidate review, explicit batch approval, and relationship-only
dry-run operations. Migration 0025 adds the targeted refresh described below.

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
and disposable-database verification. Keep the task-oriented
`docs/HOW_TO_CHORDRIFT.md`, focused `docs/how-to/` pages, and
`docs/reference/CLI_COMMANDS.md` synchronized with CLI changes;
`tests/user_docs.rs` enforces aggregate leaf-command coverage.

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
Migrations 0019 through 0025 are live and Neon is healthy at 25/25. Migration 0020
adds immutable successful managed-playlist baselines so a later missing
expected track becomes an internal `exclude_track` proposal rather than an
automatic re-add; an unexpected extra remains ordinary provider drift. The
current planner is `spotify-dry-run-v5`; earlier development plans remain
immutable audit artifacts and must not be applied. The verified v4 plan is
`cda2639d-da67-4b23-9492-a9274c71088c`, bound to approved proposal
`ca81d1b2-e56b-41e6-8846-cdb379cb039b` and Spotify snapshot
`622a94b4-b60e-4f26-8da2-20e540e160c1`. It contains 1,007 exact operations:
16 creates (14 canonical plus missing `Inbox` and `From Friends`), 884 ordered
track additions, 65 deferred intake removals, and 42 separately approved
legacy retirements; no renames, restorations, or exclusions before initial
publication. `Liked from Radio` already exists and is reused. Every inspected
retirement has complete preservation. The current v5 plan is
`68ee490c-f5f4-4e23-9a48-7f4933cd6511`, bound to Spotify snapshot
`c187fc99-5e7c-42f7-a694-86bcb9d1930b`. It contains the same 1,007 canonical
operations plus 13 separately approved `remove_external_playlist` operations,
for 1,020 total. Cleanup batch `016defcd-f46b-4070-991d-73cb4c89f00a`
captures and approves all 13 present external bookmarks with input hash
`8528685a4f488784acd5a9381d183a7795485547714981cc3d5eb25006cfaa12`.
Repeated v5 planning reuses the plan exactly. v0.0.9 apply-readiness validation
is now implemented and remains read-only against Spotify.
That milestone also generates one simple original deterministic cover per
canonical playlist from its approved name/description/tags, stores generator
version and SHA-256, produces a contact-sheet-style preview, and requires
explicit artwork approval. Do not request Spotify image-upload scope or upload
covers until v0.1.0.

Artwork implementation is now complete in source as migration 0023,
`src/artwork.rs`, and `chordrift artwork import|status|list|approve`. The
approved Drift Atlas v1 set contains 14 original 1254×1254 PNGs in
`artwork/canonical/drift-atlas-v1`, with strict `manifest.json` provenance and
`contact-sheet.png`. The user approved the 13 original candidates, requested a
darker replacement for #8 Open-Sky Anthems, then explicitly approved that
replacement and the complete set. Migration 0023 is live and Neon is healthy
at 23/23. Approved artwork batch
`450e2e83-37d5-4100-99b7-cef4a56240f5` is bound to proposal
`ca81d1b2-e56b-41e6-8846-cdb379cb039b`, contains 14 verified covers, and has
input hash `c5e295d0914f1ee8d386fcf4f7ca297e2811449cb84acbe30287afddd8d7714a`.
Re-importing the unchanged manifest reuses that exact approved batch. Artwork
approval is local/Neon-only and must not request Spotify image-upload scope or
upload covers before v0.1.0.

Migration 0024, `src/apply_readiness.rs`, and
`chordrift sync readiness|readiness-show` now persist an immutable safety
assessment for one exact plan. Live assessment
`7cedca9e-ed2b-4ddb-baca-f2a701db531c` is bound to plan
`68ee490c-f5f4-4e23-9a48-7f4933cd6511` and input hash
`575c5971219bbfc8bb3f1f8471833fadc8e19abdb16997a4d5d3d5feed0f8e91`.
It is `ready`: 10/10 checks passed across 1,020 operations, five simulated
restart checkpoints recovered all operations, and replay produced zero
changes. The one-request live probe confirmed only
`playlist-read-private`, `playlist-read-collaborative`, and
`user-library-read`; no modify or image-upload scope is granted. All 120
destructive operations remain deferred, and `spotify_writes: disabled`.

Migration 0021 established the bookmark inventory. Two consecutive
read-only pulls produced snapshots `6544a59b-c6e7-4ec0-92d8-3129132bb449`
and `c187fc99-5e7c-42f7-a694-86bcb9d1930b`: both saw 62 Spotify playlists,
kept 49 active, and retained 13 external bookmarks with no external item
requests. All 49 active playlists and 927 saved tracks reused Neon on the
second pull. `alone in the car` is bookmark Spotify ID
`1128mckrHSNSNt3PzyE4Bp`, owner `trinwoodward`, 52 reported items, status
`metadata_only`; its `last_changed_at` remained stable across the repeated
pull. It is absent from `chordrift playlists list`. The 13 followed public
bookmarks are metadata-only because Spotify Development Mode does not expose
their contents to this app; `bookmarks tracks` reports that honestly.

Migration 0025 and `chordrift bookmarks refresh` add explicit, targeted
refresh for exactly one present or archived bookmark. Refresh attempts and any
readable ordered track metadata are immutable and separate from provider
library snapshots, so they neither stale the normal sync baseline nor increase
its request budget. Spotify's February 2026 API permits playlist items only for
owned/collaborative playlists; followed public shared lists will usually record
a 403 `inaccessible` attempt while retaining their bookmark metadata and any
older readable contents. The intended workflow is: follow/save the shared list
in Spotify, pull once to bookmark it, selectively listen in Spotify, add chosen
songs to `Inbox` or `From Friends`, then run Chordrift normally. Bookmark tracks
never become semantic inputs automatically.

v0.0.9 was released on 2026-08-24 from merged `main` commit
`6580ce8f5874f1c607f0e759484d6acb80979b8d`: crates.io publication, annotated
Git tag `v0.0.9`, and the GitHub release all succeeded. Hosted CI run
`32788740035` passed formatting, clippy, all ordinary and documentation tests,
both disposable-PostgreSQL integration suites, and package verification.
Repository artwork is intentionally excluded from the crates.io archive
because the 14 approved full-resolution PNGs are review/publication assets
rather than runtime data; they remain in Git.

v0.1.0 work is on `codex/v010-spotify-apply`. Migration 0026, planner v6,
readiness v2, and `src/apply.rs` introduce the first provider-write path. Apply
is phase-scoped, requires exact assessment confirmation, persists every
operation and resolved Spotify target, batches at current API limits, resumes
against live membership, uploads only approved artwork, and stops at
`awaiting_pull`. The next pull verifies exact ordered canonical membership and
records immutable managed baselines. Cleanup/retirement additionally require
`--allow-destructive`; retirement also requires exact-plan durable approval.
No v0.1.0 live Spotify mutation has occurred yet. The existing stored OAuth
credential is read-only and must be explicitly reauthorized for the seven
documented v0.1.0 scopes before a new v6 readiness assessment can pass.
Migration 0026 is live and Neon is healthy at 26/26. Current v6 plan
`e89854e8-c1dc-42fc-b469-b7e113fcd831` is bound to snapshot
`c187fc99-5e7c-42f7-a694-86bcb9d1930b` and input hash
`520fa5b82c70fccfa3024927ad568ced0594732cecec2c0c415f2689780e7793`.
It is current and contains 1,034 operations: 16 creates, 884 additions, 14
approved artwork uploads, 65 deferred intake removals, 13 deferred external
relationship removals, and 42 deferred legacy retirements. No Spotify request
was made while creating or inspecting it.

`chordrift sync apply-preflight` now validates an exact current v6 publish plan
without contacting Spotify. The live preflight for plan
`e89854e8-c1dc-42fc-b469-b7e113fcd831` passed: all 14 approved source hashes,
PNG decodes, and JPEG conversions are valid; the largest base64 JPEG is 221,788
bytes, below Spotify's 256 KB limit. Publishing will create 16 containers,
populate 14 canonical playlists with 884 ordered memberships through 17 item
writes, and upload 14 covers. The estimated publish budget is 15 Spotify reads
and 47 writes. The preflight made zero Spotify requests. Local verification is
clean at 72 passing library tests plus the CLI/docs tests and warning-free
clippy. Hosted CI run `32791199258` completed every job step successfully,
including both disposable-PostgreSQL suites and package verification; confirm
the final GitHub status after the follow-up preflight commit is pushed.

The user completed v0.1.0 Spotify reauthorization for account `personal` on
2026-08-24. Account identity `5DPKF9q1Xm` (`suhails`) matches Neon, and the
system-keychain credential now has all seven required read, playlist-modify,
library-modify, and image-upload scopes. Read-only readiness assessment
`16c6c402-9f82-4179-8f18-f9cc24912dc9` is `ready` for exact plan
`e89854e8-c1dc-42fc-b469-b7e113fcd831`: 10/10 gates passed, all 1,034
operations recovered across five simulated restart checkpoints, and replay
produced zero changes. No Spotify write has occurred. Publishing now requires
the user to explicitly confirm that exact assessment ID before running
`sync apply --phase publish`; do not infer authorization for cleanup or
retirement from publication approval.

The user explicitly approved publication. Apply run
`35db437b-f348-434d-8402-ddde1ecb3eb8` executed all 914 publish operations
(16 creates, 884 memberships, 14 covers) with zero failures, then the pull
committed snapshot `98ec0798-c946-4b0b-bd9d-5dbf2fe64679` and reported
`verified_apply_runs: 1`. The first verification pull exposed and safely rolled
back a canonical semantic-weight constraint mismatch; no snapshot was partially
committed. The corrected importer sets canonical semantic weight to zero.
A second pre-cleanup audit caught newly created empty `Inbox` and `From Friends`
being imported with the default legacy policy. The importer now recovers intake
policy from the succeeded create operation. Read-only snapshot
`2ce8e24f-e88b-4051-8927-3501c65edc34` confirms both are protected `inbox /
provider_wins / intake / after_verified_assignment` surfaces. Current plan
`74caa6d4-8cee-40d1-a507-f8141dff5799` contains zero creates/additions, 65
deferred `Liked from Radio` removals, 13 deferred external relationship removals,
and the original 42 legacy retirements. No cleanup or retirement approval has
been inferred from publication.

The user explicitly approved cleanup. The first attempt was safely blocked
before writes because a repeated pull had a newer snapshot without a carried
verification baseline. `verify_pending_publications` now recomputes canonical
proof on every pull rather than trusting or blindly copying an older baseline.
Cleanup apply run `20f5f69c-f74a-464e-a9af-fd9643556718` then completed all 78
operations: 65 `Liked from Radio` removals plus 13 external relationship
removals, with zero failures. Spotify's playlist index briefly returned the old
snapshot marker for `Liked from Radio`; the following read-only pull observed
the new marker and exact 65-entry decrease. Destructive apply runs are now also
marked succeeded only when a later imported snapshot proves every planned track
and relationship absent. Snapshot `b9e8d29e-b409-4de4-802b-7e77f78c1d85`
reports 65 active playlists, 2,309 entries, zero followed/external playlists,
and `verified_apply_runs: 1`. All 13 external bookmark records remain in Neon as
not-present history. No legacy retirement has occurred.

The user then explicitly approved exact retirement plan
`f7c926c3-26f7-4adc-ad69-4e40d62fbf0f`. Apply run
`f767f050-ddff-4758-9a1c-6085eb9cff27` removed all 42 legacy relationships with
zero failures; snapshot `e38b7c81-9513-4d98-9d9d-9ecc73575d69` proves the live
playlist count fell from 65 to 23 and reports `verified_apply_runs: 1`. The
post-retirement audit found four documented obsolete utilities still present
because ignored/transport classes were unintentionally omitted from planner
retirement. Planner cleanup now includes `ignored` and `transport` sources in
the separately approved retirement phase while intake remains protected. Exact
plan `fa8289fc-d636-448c-8203-a8bd1ca67ae6` contains only four retirements:
`All my saved songs`, `Collaboration Jessica`, `My top tracks playlist`, and
`Two Way Sync` (plus 14 non-destructive artwork operations in publish). These
four have not been approved or removed yet.

The user explicitly approved the four utility retirements. Apply run
`ffe42fb9-7fe9-40c9-bd97-ef9e468bb9ca` completed 4/4 with zero failures, and
snapshot `5280abe1-7220-4cbb-8e9c-9acf7ef72121` verified the final live surface:
19 playlists, 951 entries, 902 unique playlist tracks, zero duplicate entries,
zero followed/external playlists, and `verified_apply_runs: 1`. The 19 are
exactly 14 canonical Chordrift playlists, `Inbox`, `From Friends`, `Liked from
Radio`, `Daily Mix`, and `On Repeat`. Current plan
`c64d615a-1fd8-4c80-afc9-08d82a42b58d` has zero creates, additions, removals,
retirements, external cleanups, or deferred destructive operations; its only 14
operations are idempotent approved artwork uploads. v0.1.0 release metadata is
being prepared; do not claim the crate/tag/release exists until publication is
verified.

v0.1.0 was released on 2026-08-24 from merged `main` commit
`2968af5`: PR #3 merged with full audited history, hosted CI run `32795061524`
passed formatting, warning-free clippy, all ordinary and documentation tests,
both disposable-PostgreSQL integration suites, credential persistence, and
package verification. `chordrift 0.1.0` is published on crates.io. Annotated Git
tag `v0.1.0` was pushed, and the non-draft, non-prerelease GitHub release is
available at `https://github.com/orbyts/chordrift/releases/tag/v0.1.0`.

Apogee configures a machine-wide shared Cargo target. Because the released
`$CRATES/chordrift` clone and this development workspace currently share the
same package name/version, a plain `cargo run` reused an older final executable
that lacked `proposals`. For unreleased development commands, use a repository-
specific target such as `cargo run --target-dir target -- ...`; do not modify
shell initialization files.

## v0.1.1 task state

Work is on `codex/v0.1.1-track-inspection-artwork`. The task adds
`chordrift tracks inspect --name TITLE [--artist ARTIST]` and
`--spotify-id ID` as a single explainability report spanning current Spotify
placement, approved canonical destination and provenance, all retained source
playlist observations, listening/lifecycle signals, embedding generation and
dimensions, cluster similarity/rank, independent semantic model facts, manual
assignment reasons, and active exclusions.

Migration 0027 allows approved artwork artifacts to target canonical or intake
surfaces, including an intake that has not been created yet. Planner v7 adds
`From Prompts` with prompted-interest semantics and suppresses an artwork upload
when the exact content-addressed operation already succeeded against the same
stable Spotify playlist ID. Apply target resolution can bind a newly created
intake cover by its unique planned name.

Drift Atlas v3 lives at `artwork/canonical/drift-atlas-v3`: 14 previously
approved backgrounds remain visually unchanged and four intake backgrounds
cover `Inbox`, `From Friends`, `Liked from Radio`, and `From Prompts`. All 18
pristine label-free masters remain in the `backgrounds/` child directory for a
future Apple Music typography pass. `scripts/render_artwork_label.swift`
performs the exact 1254×1254 CoreText overlay; AI-generated text is never used.
At user review, Helvetica Neue Bold increased to Spotify-like 116/132-point
labels measured and anchored 42 pixels from the lower edge. The schema-2 v3
manifest contains exact hashes and provenance summaries.

The v0.1.0 preservation gate was discovered to be too narrow: it counted only
699 current semantic-legacy/intake tracks, while the durable account inventory
contains 1,711 distinct tracks. Migration 0028 is live in Neon (28/28 healthy)
and defines the account-scoped preservation universe as latest saved tracks plus
all historical semantic-legacy, transport, intake, and canonical membership,
with active reversible exclusions as the only alternate disposition. Raw
listening history and provider-curated playlists remain enrichment signals and
do not enlarge the library. Readiness v4 dynamically requires exactly one
disposition per inventory track, so an older stored coverage flag cannot bypass
the corrected invariant for this or any future account.

The current editable proposal is `fcfaccc7-e17d-4dee-a54c-65a73000fcc2`.
It preserves all 14 approved concepts, names, descriptions, tags, artwork
identities, and the original 884 placements, then adds the missing inventory
through direct centroid fit, analytical-cluster group consensus, listening-
session context, and explicit account-scoped manual decisions. Exact live Neon
audit: 1,711 inventory, 1,711 placed, 0 excluded, 0 unresolved, 0 conflicting.
Source-class audits are also complete: 927/927 saved, 674/674 semantic legacy,
143/143 transport, 65/65 intake, and 884/884 prior canonical tracks. The
proposal is fully named and `coverage_complete=true`. The user approved and
published this exact generation; its 1,711-track managed baseline is verified
against Spotify.

Embedding generation `baf6d7af-0333-461b-a72d-7392e315357f` is model
`semantic-feature-hash@4`, 1,024 dimensions, and embeds 1,688/1,711 inventory
tracks. v4 adds normalized 45-minute meaningful-listening-session
co-occurrence; 1,159 tracks share session context and the unembedded tail fell
from 173 to 23. Analytical cluster generation
`180b4b87-fbff-4c42-90ce-76b853550f2a` has 18 groups and zero cluster-level
unassigned embedded tracks. Group consensus required at least 10 known members
and 55% destination dominance; every automated membership records its exact
embedding/cluster generation, score, counts, and threshold.

`tracks inspect` now reads the newest proposal as well as approved/published
state. `Do Your Best` by John Maus is explicitly assigned to `Neon Affection`
with the reason that its lo-fi synth-pop/nocturnal post-punk character is a
better fit than its borderline 0.0504 Open-Sky centroid result. The report also
shows 64 meaningful plays, 72 events, 12 skips, 2.79 hours, the v4 embedding,
analytical cluster, retired `Two Way Sync` provenance, and the manual override.

New inventory/repair commands are documented and enforced by the CLI-doc test:
`proposals inventory`, `unresolved`, `placement-audit`, `extend`,
`group-tracks`, `consensus-assign`, and `centroid-assign`. The pending Drift
Atlas v2 batch `f1430424-8c71-4210-86cb-07adf4eb17ff` targets the old proposal
`ca81d1b2-e56b-41e6-8846-cdb379cb039b`; do not approve it. The local manifest
is rebound to approved complete proposal
`fcfaccc7-e17d-4dee-a54c-65a73000fcc2` without changing any of the 18 image
bytes or hashes. Its replacement immutable artwork batch is
`6587f24a-999d-4b88-a97b-2a1bfe49c425`, is `approved`, contains 18 artifacts,
and has input hash
`f9151ca22c887456abfdc4fe02720f1ca6db2ba88dd388b2cbd63017c033a1c9`.
The user approved it at `2026-08-25T02:46:01.285564+00:00`; none of its image
bytes changed after approval. It was published as the initial v2 surface
and was later superseded by the approved v3 typography pass described below.

Historical complete-library v7 dry-run plan
`67e0b557-126e-4a60-ba11-676caffe85ff` targets approved proposal
`fcfaccc7-e17d-4dee-a54c-65a73000fcc2` and current source snapshot
`cf361d5b-1f9c-4ca8-8ec3-d716d8351283`. It contains 846 wholly
non-destructive publish operations: one intake create, 827 additions, and 18
artwork uploads, with zero removals, retirements, exclusions, external cleanup,
or deferred operations. Offline preflight passed: 13 populated playlists, 18
batched item writes, all 18 covers valid, largest converted cover 224,456
bytes, estimated 14 Spotify reads and 37 writes, and zero Spotify requests
made. Plan input hash is
`6d089c22cec40a674c249e0a9c48c9c53217457c369188dac4af431292a101d9`.

The user reauthorized the unreleased binary for account `5DPKF9q1Xm`
(`suhails`) with all seven required scopes. Membership apply run
`af9e3265-a0ce-486f-94df-4a0cc3256414` executed 846/846 operations with zero
failures and is verified `succeeded`: one `From Prompts` intake create, 827
membership additions, and 18 v2 cover uploads. Import snapshot
`9a9a4fb6-097a-4315-9938-385605a46dc8` established 20 owned playlists, 1,778
entries, 1,727 unique playlist tracks, zero duplicate entries, and the exact
1,711-track canonical baseline.

Two post-publication verifier defects were fixed in `src/apply.rs`: approved
empty canonical playlists must participate in desired-state comparison, and
sparse proposal ordering keys must be compared as ordered track-ID sequences
rather than raw positions because Spotify densifies positions from zero. The
first complete verification exposed both safely; no membership was lost or
rewritten while diagnosing them.

Drift Atlas v3 batch `776ae100-f16b-477d-838d-8b90cfda9e6e` is approved with
input hash `06de03171089f7c3dba0116709a275090850a082af01d29d31e9722420862ae0`.
Cover-only plan `d4b78b32-9f82-4717-9891-0d93b4855879` passed preflight and
11/11 readiness gates. Apply run `7031e284-a9e9-4b01-963c-84a735d36d46`
uploaded 18/18 larger lower-anchored covers with zero failures and is verified
`succeeded` against snapshot `3cc2d1c0-6f1b-42dc-8cff-2b6e9b952567`.
Planner artwork selection now uses only the newest approved batch, preventing
stale approved revisions from entering a later plan.

`chordrift artwork update --account personal --playlist NAME_OR_STABLE_KEY`
now builds a focused immutable one-cover plan from the newest approved batch.
It refuses missing, ambiguous, unresolved, or already-uploaded artifacts; the
existing preflight/readiness/apply flow remains the provider-write boundary.
Spotify playlist folders and folder covers are unavailable through the Web API
and remain manual, provider-controlled presentation state.

The earlier repair verification was clean and the live Spotify membership and
v3 artwork publications described above completed successfully. The final
v0.1.1 release verification is recorded below.

## v0.1.1 final working model

Normal `sync pull` now incrementally retains Spotify Recently Played events
after a durable Neon cursor. These API observations are provisional because
Spotify does not supply playback duration, completion, or skip facts; a later
cumulative Extended Streaming History import supersedes overlapping API rows
before rebuilding listening statistics. Migrations `0029` through `0031` add
the incremental-history cursor/source model, protected `user_managed` playlist
defaults, and exact playlist-order replacement support.

Spotify OAuth uses one consolidated PKCE consent. Chordrift no longer rewrites
an unchanged Keychain credential on every command; the earlier repeated prompts
were caused by an unconditional credential write compounded by changing unsigned
debug binaries. A stable signed build remains the correct friend-test delivery.

User-owned playlists now default to protected and retirement defaults to none.
`chordrift playlists retirement` can include exact names, select all with
explicit exceptions, or reset to none. This changes Neon intent only; complete
coverage, immutable-plan inspection, exact approval, readiness, and
`--allow-destructive` remain mandatory before a provider write.

Live v0.1.1 convergence completed on 2026-08-24. Publish apply
`2496d685-eb6f-485f-a6ec-0b7d19705290` repaired the exact order of three
canonical playlists. Retirement plan `558a6cc7-2be8-4519-9208-f048a759430b`
and assessment `72f7ed23-88d2-4afa-ba55-bd276baac506` passed 11/11 checks and
retired only the explicitly approved user-owned duplicate `On Repeat`
(`0z02mUNjp2VHfZIjt7Iuhm`). The post-retirement snapshot
`08469f65-6095-430c-84b7-281d8725aa02` contains 19 playlists, 1,752 entries,
1,727 unique playlist tracks, 927 saved tracks, and zero duplicate entries.
The unique-track count stayed at 1,727 while 30 duplicate playlist entries
disappeared, proving no song was lost.

Listening history is current through `2026-08-25T04:46:07.125Z`: 149,249
retained events, 15,563 unique historical tracks, and 6,602.55 listening hours.
The complete-library readiness gate covers all 1,715 preserved inventory tracks
with zero unresolved or conflicting dispositions.

Final local release verification: 77 ordinary library tests pass, one
PostgreSQL-only library test is expectedly ignored locally, the user-doc
coverage test passes, formatting is clean, clippy is warning-free across all
targets and features with `-D warnings`, and `cargo package --allow-dirty`
successfully verifies 80 packaged files. The live disposable-PostgreSQL suites
remain hosted-CI responsibilities.

## v0.1.2 listening-review decision

The next milestone is an ongoing listening-review loop, not a fresh global
recluster. The user has already noticed placements that may be acoustically
plausible but personally wrong—for example, some A. R. Rahman or other South
Asian recordings inside `Tidal Hush`. Preserve four distinct intents: reject a
destination, prefer/lock another destination, hold for review, or exclude from
active Chordrift playlists. Prior assignments and scores remain auditable.

Build a first-class review session over a cloned approved generation, with
ranked alternatives, batch correction, coherent new-playlist promotion, exact
diff approval, normal sync readiness/apply, and post-pull convergence. Keep
track-specific feedback as a hard constraint. Region, culture, language,
soundtrack context, instrumentation, and mood are separate facets; never infer
a universal placement rule from an artist name or one correction. Repeated
consistent corrections may produce an evidence-backed rule suggestion that
still requires approval.

Direct edits to Chordrift-managed Spotify playlists should be staged as
possible feedback by comparing them with the last verified baseline. A remove
plus add suggests a move; remove-only is ambiguous among wrong vibe, review,
and exclusion; an addition suggests destination preference; reorder is an
ordering-policy question. Never silently learn or reverse ambiguous edits.
This allows Spotify to remain the familiar consumer editing surface while
Chordrift acts as the preservation-first assistant and Neon remains the durable
ledger. The complete design is in the v0.1.2 section of `ROADMAP.md`.

The user clarified that correction capture happens during ordinary listening,
not in a dedicated review session. v0.1.2 therefore separates capture from
reconciliation. The stable action intents are Refile (keep, wrong destination),
Review (keep, undecided), and Exclude (proposed reversible removal). Optional
Spotify routing queues such as `Route — South Indian`, `Route — North Indian`,
and `Route — Decide Later` provide the lowest-friction mobile action. Adding a
track is sufficient; Chordrift records it on pull, publishes a verified
destination before removing the rejected membership, and clears the queue only
after convergence. Queue descriptions are semantic data, not commands.

Concrete motivating example: `Tidal Hush` positions 28–33. The working North
Indian/Hindi cohort is `Chhodo More Baiyyan`, `Ni Main Samajh Gayee`, and
`Kahin Aag Lage`; the working South Indian cohort is Tamil tracks `Kandukondain
Kandukondain` and `Senkathay`, plus Telugu `Thongi Choose`. These are routing
labels; user-approved poetic playlist names and artwork can follow later.

## v0.1.2 routing-playlist subslice

Branch `codex/v0.1.2-listening-review` now contains migrations `0032`–`0033`, a generic
`chordrift routes` CLI, zero-signal route capture during Spotify import, v9
publish planning, route artwork operations, and exact route verification after
pull. A route is a transient corrective inbox: non-empty means pending work;
it must be cleared only after an existing or newly created canonical destination
has been published and verified. Route descriptions are retained semantic
policy data, not executable instructions.

Approved initial routes are `Route — South Indian`, `Route — North Indian`,
and `Route — Decide Later`. Their label-free masters and deterministic Spotify
overlays live under `artwork/routing/route-signals-v1/`. The two regional covers
are intentionally minimal representational instrument studies (veena and
sarod); Decide Later is a minimal junction. This is not a fixed visual template:
future routes need artwork designed for their own meaning and should retain
both the pristine master and provider-specific label render.

Initial desired memberships from `Tidal Hush` are North/Hindi Spotify IDs
`4izSWLwW0wQohWBuEUKL5J`, `7wJphnipgpptcsRk6Aur3w`, and
`3AemMBXKJWFd87svnFyrHy`; South Indian IDs are
`42lDp1YYCiy50UtXUO9FNp`, `1VdBV90HgsUkjdKo95qnLf`, and
`2m7cVrIHAfJmZhCCOZ91qT`. They remain in their current canonical playlist until
the later review/reconciliation step establishes verified poetic destinations.

The first post-publication pull exposed and atomically rolled back an older
clear-policy constraint; migration `0033` now permits verified clearing for
`routing` as well as `intake`. The route snapshots then imported exactly. A
second verification refinement scopes each apply run to the surfaces it
actually touched, so a route-only publication proves route convergence without
being coupled to unrelated canonical-baseline verification.

Live routing publication completed on 2026-08-24 through plan
`3c0268b2-02a5-4148-b1ae-c74f82f6fa0e`, readiness assessment
`a3441c60-bfc5-4b45-8ab8-4c99b371087c` (11/11 checks), and apply run
`e50996f4-a25c-4b42-b12b-b96a73c91447` (12/12 operations succeeded and
post-pull verified). Spotify IDs are `0PrQn0SnXQ7azykCR0Y6PW` for Decide Later,
`11qUBVVOuiKeR4uD6RdO46` for North Indian, and
`04DgVypEVHs0fkmoMBLkgB` for South Indian. Snapshot
`75c91908-b155-49ec-87e4-76a677fe9ca3` reuses all 22 playlist bodies and proves
1,758 entries, 1,727 unique tracks, and zero duplicate entries. The unique
universe is unchanged; the six route entries are preserved review duplicates.
The subsequent v9 plan `88b31b13-f1c8-4a24-960b-6e68ca64a350` contains zero
operations, proving route creation, exact order, and artwork publication are
idempotently converged.

The user documentation is now split by intent. `docs/HOW_TO_CHORDRIFT.md` is an
80-line entry point and table of contents; the former comprehensive guide is
preserved as `docs/reference/CLI_COMMANDS.md`. Focused pages under
`docs/how-to/` cover adding/discovery, deleting/excluding,
routing/reclassification, and sync/convergence. The product-facing inference
rules live in `docs/design/PLATFORM_INTENT_MODEL.md`: high-confidence provider
actions may be captured automatically, ambiguous actions must be staged, and
destructive interpretations are never silently inferred.

## v0.1.2 incremental-sync and terminal subslice

A live pull after one saved-track deletion changed Spotify's reported total
from 928 to 927. Spotify retrieval completed, but persistence remained silent
for roughly six minutes because the importer sequentially rewrote existing
track, album, and artist metadata and inserted each saved membership through a
remote Neon round trip. The pull eventually succeeded as snapshot
`c3c4cbba-7c33-4c52-9573-cbfa9b5cbfac`: 22 playlists, 1,766 playlist entries,
927 provider saved items, 926 supported saved tracks, one unsupported item, and
two new Recently Played observations.

The branch now preloads known Spotify track rows once, compares the retained
JSON payload, skips metadata writes for identical records, batches saved-track
snapshot membership in groups of 1,000, and updates `last_seen_at` for observed
provider rows with one set-based statement. The full persistence transaction
and unsupported-item accounting remain intact. `indicatif` provides TTY-only
progress bars with plain redirected fallbacks. `comfy-table` provides compact
colored interactive tables for `playlists list` and `playlists tracks`; the
existing complete TSV remains unchanged when stdout is redirected. An actual
80-column Neon-backed rendering check led to a four-column stacked playlist
layout instead of an unreadable ten-column grid.

The optimized branch was labeled and locally installed on 2026-08-25 as
`chordrift 0.1.2-dev.1`. This is an intentionally truthful prerelease: deletion
to staged exclusion, intake capture, routing capture, progress bars, and compact
playlist/song tables work, but route reconciliation is not release-complete.
Until it is, a wrong-destination track must be added to a route without also
being removed from its canonical playlist; canonical removal currently means
exclusion. Different tracks may carry deletion, discovery, and routing intent
in the same pull, but one track must not express conflicting intents.

The next named checkpoint is `0.1.2-dev.2`. It adds revisioned private track
classification, safe direct/CSV review paths, a sectioned interactive track
inspection report, opt-in `--technical` provenance, and full-width interactive
tables. Stable `0.1.2` remains gated on route reconciliation and the verified
Monsoon Cinema regional split.

Remaining v0.1.2 gates are: clone the approved library into a focused review
draft without rebuilding unaffected playlists; turn route captures into durable
negative/current-destination constraints; assign route or intake tracks to an
existing canonical concept or promote a coherent cohort into a new poetic
concept; integrate name and artwork approval for that new concept; publish and
verify the destination before removing the old membership; consume both the
Spotify route entry and its durable Neon desired-route membership so it cannot
be restored; and test mixed deletion/routing/discovery cycles end to end. A
route is not necessarily one future playlist: the regional route is a review
facet and may feed several sound-coherent poetic destinations.

## v0.1.2 saved-library reliability subslice

The user paused cleanup until Neon, Spotify, and code convergence are proven.
Two defects were identified and repaired: active exclusions now supersede stale
approved-proposal memberships in readiness accounting, and awaiting reconcile
runs verify both the durable exclusion ledger and provider absence against the
latest pull before succeeding.

Migration `0034_saved_album_inventory.sql` adds immutable saved-album and
ordered album-track snapshots plus account-scoped policies. Normal Spotify
inventory now probes `/me/albums`, copies an unchanged snapshot from Neon, and
otherwise persists complete album track membership. Album-only tracks remain a
separate review surface and do not block playlist readiness. New read-only
commands are `albums list`, `albums audit`, and `albums tracks`; `albums policy`
defaults to preserve and can opt the personal account into review-then-unsave.

Spotify Liked Songs (`/me/tracks`) is now defined as the primary easy intake:
Like means keep and classify. `spotify library-policy --liked-songs
clear-after-verified-assignment` opts an account into gated cleanup only after
verified canonical placement or durable exclusion. The planner emits explicit
deferred `remove_saved_track` cleanup operations; execution requires readiness,
exact confirmation, and `--allow-destructive`, and post-pull verification
requires absence from the newest saved-track snapshot. The default remains
preserve for every account.

Live validation on 2026-08-25 exposed and repaired two additional shared-state
bugs. Post-publish verification had incorrectly recognized only internal
playlist kind `canonical`; approved Chordrift outputs are `generated` or
`manual`. It now recognizes proposal-backed playlists and compares effective
membership after active exclusions. The publish executor also previously read
raw proposal membership during an exact reorder, temporarily restoring 48
excluded Monsoon Cinema tracks. It now uses the same account exclusion filter
as planning/readiness/verification. The 48 entries were removed through exact
plan `a13897d0-4d2b-4247-8323-7ea1c145fe8e`, readiness assessment
`da486e32-b670-4e8c-a7e9-c03d59ddf9ba` (11/11), and verified reconcile run
`5a6e03a8-e45e-4b76-a32e-4902ba940996`.

Changed playlist and saved-album memberships are now batched into Neon. Album
track persistence reuses known full provider identities instead of overwriting
them with simplified album-track payloads, and album-only response fields are
excluded from embedded track metadata serialization to keep comparisons
stable. A second pull proved all 69 albums and 657 album tracks copy forward
from Neon after a one-page signature probe.

The live converged snapshot is `3fac64c9-6b08-47fe-b9a1-0a02ebfe43b4`: 22
playlists, 1,685 entries, 1,622 unique playlist tracks, 923 supported saved
tracks, 69 saved albums, 657 album tracks, and zero duplicate playlist entries.
Fresh plan `9d74e245-180c-4834-8965-720b484d9ac9` contains zero operations. The
album audit reports 485 distinct tracks already preserved in Liked Songs or a
current playlist, 69 explicitly excluded, and 103 awaiting review across six
albums. Both personal policies remain `preserve`; do not enable cleanup until
the user reviews those 103 tracks.

The user subsequently chose an archive-only personal album policy: retire all
saved-album containers while retaining immutable album and ordered-track
history, without forcing album-only tracks into playlists. Migration 0035 and
planner v10 implement exact retirement operations, separately approved apply,
and post-pull verification. The product default remains `preserve`.

Live album retirement completed on 2026-08-25. Exact plan
`89173e61-35a5-43a1-a837-8479a310094a` contained 69 container-only operations;
assessment `84f94a04-77b9-4845-8189-81b72592ebc8` passed 11/11 checks. Apply run
`4958307f-8df1-40fb-85ee-2b7672115ec0` removed all 69 Spotify album containers
with zero failures and verified against snapshot
`63f13a92-434f-4c90-b5f6-e432221f0da3`. That snapshot has zero saved albums,
while `albums history` retains all 69 as retired. Fresh plan
`6114972d-86d1-437f-877e-084c777d5a1a` has zero operations. Next work is route
reconciliation, followed by verified Liked Songs consumption.

## v0.1.2 Monsoon/regional reconciliation audit

The next requested cleanup retires mixed destination `Monsoon Cinema` only
after lossless reassignment. Snapshot `63f13a92-434f-4c90-b5f6-e432221f0da3`
contains 410 Monsoon tracks across 112 albums. The three route surfaces contain
75 entries / 73 unique tracks, but only 29 unique routed tracks are in Monsoon;
44 routed tracks currently live in other managed destinations. Two tracks are
present in both North and South routes. Therefore reconciliation must cover the
whole approved proposal, not merely split Monsoon.

Existing provider metadata is identity-rich but region-poor: the enrichment
ledger has 39 semantic facts and only two tracks with artist-area facts across
1,715 eligible tracks. Spotify supplies title, artist, album, ISRC, duration,
and release metadata but no reliable track-language field. Use explicit routing
as high-priority evidence, album/version evidence where unambiguous, and a
reviewable semantic classification artifact for the remainder. Indian
Classical is a style destination and requires positive classical evidence; do
not infer it from artist nationality or an instrumental title. Non-South-Asian
tracks return to existing sound-based poetic destinations, with a new poetic
destination created only when their sound warrants it. Route cleanup requires
a newer durable assignment and verified destination; current mere placement in
the rejected destination is insufficient.

## v0.1.2 final reconciliation and Re-evaluate handoff (2026-08-25)

Work is on `codex/v0.1.2-listening-review`. Reviewed CSV
`/Users/suhail/Downloads/south-asian-classification-completed.csv` activated as
classification batch `01256e29-40d3-4ade-a9a5-74e79dad252b`; its exact backup
is `south-asian-classification-completed.before-chordrift-v0.1.2.csv`.
Embedding generation `6b726259-9027-411c-84a2-baedebeebcd9` feeds editable
proposal `f521e707-8e5f-4283-a0bd-d123df3329f1`.

The proposal currently represents 1,754 required tracks exactly once, with 106
durable exclusions, zero unresolved, zero conflicts, and no missing retirement
coverage. `Latika's Theme` (`1Nnrj856MMGbtVRzJBMTFE`) is reversibly excluded as
provider-unavailable. `Monsoon Cinema` is absent from the draft. New concepts:

- `Dakshina Pulse` (`playlist-a101297467c8`): reviewed South Indian cinema plus
  the new Tamil Inbox tracks;
- `Uttara Glow` (`playlist-52a3435dea63`): reviewed North Indian cinema plus
  `Rihaayi De`;
- `Rasa Archive` (`playlist-93b60e96e38d`): the intentionally personal,
  nostalgic South Asian cross-section.

A final CSV-versus-proposal audit found and corrected centroid spillover. All 17
globally classified A. R. Rahman/Rachel Portman film-score cues plus `Unborn
Children` now belong to `Afterlight Score`; LCD Soundsystem's `Someone Great`
belongs to `Midnight Niagara`. These are explicit assignment revisions, not
new cultural claims.

The old `Route — South Indian`, `Route — North Indian`, and `Route — Decide
Later` workflow is superseded by one `Re-evaluate` queue. Migration 0039 adds
the surface purpose and immutable entry/exit ledger. Pulls capture the rejected
source concept. Planning suppresses exclusion/restoration while queued and
clears a track only when an explicit assignment revision newer than queue entry
targets a different concept. `reevaluate retire-legacy --confirm "RETIRE LEGACY
ROUTES"` is Neon-only, requires replacement-queue existence and complete
coverage, and makes the later plan archive the old Spotify containers.

The user approved all four new covers. The complete Drift Atlas v4 batch lives
in `artwork/canonical/drift-atlas-v4/`: 16 canonical and four intake artifacts,
plus preserved label-free backgrounds and `contact-sheet.png`. Artwork batch
`e1e7697a-def7-414d-8835-981af018b059` is imported and approved with input hash
`6bde7f1743b9ee06c1d526cd44e1de58d7f956483cb176a957acf07880ff0abc`.
The approved Re-evaluate cover is also retained under `artwork/review/`.

The Neon-only Re-evaluate surface is `0f81d8ea-3b63-4b23-bf0e-c6d8c52d02dd`.
All three legacy route records are inactive and retain coverage history. The
proposal is approved. Planner v10 now explicitly retires any current managed
canonical concept absent from a complete approved proposal; a regression test
protects this invariant so `Monsoon Cinema` cannot survive silently.

The final immutable plan is `9d3fdc18-1ae5-48ef-a9bd-00d1e0a4b3a9`, sourced
from current snapshot `69b80f1a-0c20-4834-9f51-b7a02cfd1c18`. Its 799 exact
operations are: four playlist creates, 598 additions, four artwork uploads, 189
removals, and four container retirements. The removals are exactly 145 consumed
Inbox entries and 44 managed-provider-drift corrections; there are no
exclusions, external cleanups, unexplained removal reasons, renames, reorders,
or restorations. Retirements are `Monsoon Cinema`, `Route — Decide Later`,
`Route — North Indian`, and `Route — South Indian`. Exact retirement approval
is recorded.

Provider-free preflight passed: 1,754 playlist entries, 26 batched item writes,
four artwork uploads, and an estimated 34 Spotify writes. Final readiness
assessment `78dcee8c-055e-4508-937c-d9b4fbd22af2` passed 11/11 checks, including
the authenticated read-only identity/scope probe, interruption recovery, and
idempotent replay. No Spotify write has been made. Stop here until the user
explicitly approves applying this exact plan; do not regenerate it or pull a
new snapshot first, because that would invalidate the audited readiness.

## v0.1.2 live Spotify convergence (2026-08-25)

The user approved execution. Publish run
`a6d8b47d-1b1d-4220-9b64-4fb026971f30` completed all 606 operations with zero
failures. Its exact-order publication batches also removed the 44 managed-drift
memberships while placing all 598 approved additions. Verification exposed two
edge bugs, both fixed and covered by the full test/lint gate before proceeding:

- Re-evaluate mirroring named an unconditional `(playlist_id, track_id)`
  conflict target even though only generated memberships have that partial
  uniqueness rule. The queue is provider-owned and already cleared before
  mirroring, so the replacement insert now correctly has no conflict clause.
- Canonical verification included separately approved legacy concepts, creating
  a cycle where Monsoon had to disappear before the retirement phase was
  allowed. Verification now compares exactly the concepts in the approved
  proposal while retaining legacy containers until their gated retirement.

Canonical publication verified against snapshot
`e41b7e51-f5d0-4f37-ad1e-8253c348749c`. Cleanup run
`6e853243-8c5b-44d1-a990-abbc257a2a3f` removed exactly 145 consumed Inbox
entries with zero failures; Spotify required one propagation refresh before
snapshot `068aeb95-becb-46bb-b21e-5fa72ff5834a` verified the absences.
Retirement-only plan `fa017618-90b8-4b6b-a62a-fec86c276cc9` and readiness
assessment `d6ead94b-728c-4a8a-b896-cdd4861f649c` passed 11/11 checks.
Retirement run `99d95649-3b99-4138-9bba-db5c2312a224` removed the Spotify
library relationships for Monsoon Cinema and all three legacy routes, retaining
their Neon inventory and history, with zero failures.

Final snapshot `ca779284-b161-48e2-bc9a-bde5f0be640b` has 22 playlists, 1,791
playlist entries, 1,766 unique playlist tracks, no duplicate entries, 346
supported saved tracks plus one unavailable/unsupported saved item, and zero
saved albums. Final convergence plan
`66994c42-5f5c-4f31-8b2d-85b17ac81dd3` has zero operations in every phase.
