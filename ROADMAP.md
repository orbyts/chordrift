# Roadmap

Chordrift will be developed in small, auditable milestones. Neon is the source
of truth throughout, while Spotify and Apple Music remain provider adapters.

The Spotify downloadable history archive is an optional, independently
importable enrichment source. No other milestone is blocked on receiving it.

## Discovery and orchestration model

The guiding product principle is **a clean listening surface backed by lossless
musical memory**. Every active playlist should have an intentional purpose;
every retained track should have inspectable provenance explaining whether it
was a personal favorite, an old playlist member, a provider discovery, a friend
recommendation, or part of an external followed playlist. Cleanup must preserve
the best available history before removing clutter and explicitly identify any
provider data that could not be recovered.

The resulting canonical playlists should combine current high-rotation music,
forgotten favorites, new discoveries, and explicit recommendations according
to an inspectable composition policy. Their approved names and artwork should
make the final provider surface inviting enough to replace defaulting to radio,
while native radio and discovery continue supplying new intake.

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

Beyond canonical organization, Chordrift will generate renewable listening
playlists from versioned recipes. Canonical collections answer “what is this and
where does it belong?”; intake surfaces answer “how did it arrive or what needs
review?”; generated playlists answer “what would be rewarding to hear now?” A
track may participate in several generated playlists without duplicating or
weakening its canonical identity.

Recipes combine eligible source sets with composition weights, hard
constraints, ordering policies, and a repetition budget. Initial dimensions
include recent discovery, recent rotation, long-term favorites, forgotten
favorites, explicit recommendations, canonical diversity, artist spacing,
duration, energy flow, cross-output reuse, and user-defined sections. Curated
presets and advanced controls compile to the same versioned recipe model. Each
generation records its inputs, evidence capabilities, recipe version,
constraints, random seed, selection reasons, and final order.

Provider adapters expose capabilities rather than forcing provider-specific
logic into recipes. A feature may be available immediately, improve as normal
syncs accumulate observations, require an optional archive, or remain
unsupported. The UI must say which case applies. Spotify saved timestamps
support new-discovery recipes; Recently Played provides bounded short-term
evidence; the optional extended-history export enables trustworthy lifetime
rotation, skip/completion behavior, and deep rediscovery.

User intent has explicit strength. Hard boundaries cannot be crossed by a
normal recipe; strong preferences affect eligibility; soft facts affect rank;
one-time choices affect only one generation. Approved corrections are
account-scoped learning evidence. Chordrift may propose broader rules from
repeated corrections, but must explain and obtain approval before activation.

For every account except an explicitly selected migration, the default is
**retire none**. User-created playlists are protected, remain provider-owned,
and retain their custom names and exact ordering in Neon. Users may opt named
playlists into retirement, select all with explicit exclusions, or reset to
none; those policy commands never mutate a provider and do not bypass coverage,
review, readiness, or destructive-apply gates.

Stable user-managed intake names are `Inbox` for direct personal discoveries,
`From Friends` for explicit recommendations, `Liked from Radio` for
radio/autoplay discoveries, and `From Prompts` for tracks intentionally carried
forward from Spotify prompt-generated playlists. Spotify manages `On Repeat`,
`Daily Mix`, and the source prompted playlists. Chordrift-managed outputs receive approved generated vibe
names and must not be edited as intake surfaces. The temporary Atmos workaround
uses `Chordrift Spatial Audio`.

Suhail's explicitly approved one-time cleanup targets a final Spotify surface
containing only those four user-managed
intakes, Spotify-managed sources, Chordrift-managed canonical playlists, and
the temporary `Chordrift Spatial Audio` companion. All other user-created
legacy vibe and utility playlists are retirement candidates once their
semantic evidence has been consumed and every track has a published, verified
canonical destination. This explicitly includes `Melodi(es)` and
`Ambient Music Therapy – Indian Lounge - Relaxing Music for your Six Senses`.
Retirement removes the old playlist container, not its tracks from the library.
Spotify Liked Songs is a provider library surface rather than a playlist and is
not part of legacy-playlist retirement. It is also the primary low-friction
intake action: Like means “keep and classify.” The safe account default keeps
Liked Songs intact. An explicit account policy may instead consume each saved
track only after its canonical playlist placement or durable exclusion is
published and verified; Neon retains the original saved timestamp and history.

## v0.0.0 — Namespace reservation

Reserve the crate and repository names with a minimal, dependency-free package.

## v0.0.1 — Project skeleton and Storexa

Add configuration, CLI boundaries, Storexa-backed Neon access, migrations, and
the canonical schema. Provide version and database-status commands.

Status: complete.

The post-v0.1.1 listening path combines one lifetime extended-history baseline
with cursor-based Recently Played ingestion during every normal pull. Annual
cumulative exports supersede overlapping provisional observations and repair
any gaps without duplicating events.

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

## Deferred provider track — Apple Music

Apple Music is not on the critical path to the canonical library. Its provider
foundation is isolated on the `codex/apple-music` branch until paid Apple
Developer Program access is independently worthwhile and the integration can
be tested with real credentials. When resumed, add MusicKit authorization,
ISRC-first catalog matching with scored metadata fallbacks, unresolved-match
reporting, and optional privacy-export history ingestion. Keep all operations
read-only until the normal synchronization approval milestone.

Spotify and Neon remain sufficient to develop and validate embeddings,
clustering, naming, playlist proposals, and dry-run synchronization. Apple work
will be rebased onto the then-current main line rather than reserving or
blocking a release number.

Neon remains the durable identity, provenance, history, and orchestration
ledger; Spotify is the only active live provider and intake surface until
native Apple support resumes. Bootstrap the existing Apple library once from
SongShift JSON rather than creating temporary Spotify playlists. Retain the
original exports in the Git-ignored content-addressed archive, automatically
link only unambiguous identities, and stage uncertain metadata matches for
review. After consolidation, SongShift can mirror multiple Chordrift-managed
Spotify playlists to Apple Music using the same approved canonical names. No
aggregate "two way sync" or transfer-relay playlist is required.

The normal Neon library surface is a live projection of the latest successful
provider snapshot: only current Spotify playlists and their current names are
active. Older names and removed playlists remain solely in immutable sync/audit
history. Proposed Chordrift playlists remain separate until approved and
published.

Provider membership and playlist ownership are separate facts. A playlist
owned by another person or organization is an **External Playlist Bookmark**
when it is followed, added to the library, or externally collaborative. It is
not part of the active canonical library, contributes no clustering or
behavioral signal by default, and is never a legacy-retirement source. Before
account cleanup, Chordrift should retain its provider, stable ID, owner, public
link, relationship, last-known metadata, and an immutable content snapshot when
the provider permits access. If contents are inaccessible, record that fact
rather than pretending the bookmark is complete.

Account cleanup may then propose removing the external playlist from the
user's provider library without deleting or modifying the original owner's
playlist. This is always an explicitly approved operation. Neon retains the
bookmark and last-known contents afterward so the user can inspect or revisit
it even though the provider account surface stays clean. The same
provider-neutral distinction applies to Spotify and Apple Music.

When both native providers are active, each platform is authoritative evidence
for user actions on that platform. A user removal creates a provider-scoped
tombstone/override; it does not erase the canonical track, history, or
provenance from Neon. Reconciliation policy decides whether an intentional
removal propagates to other providers and must prevent deletion/re-addition
loops.

Chordrift also maintains a durable, provider-neutral **Excluded Tracks** view.
Removing a track from a published and subsequently verified Chordrift-managed
playlist records a reversible account-level exclusion with its provider,
timestamp, and previous canonical assignment. It must not hard-delete the track
or listening history, and it prevents the track from silently reappearing in a
future generated playlist. Removal from provider-curated, intake, transport, or
legacy playlists is ordinary source drift and must not create a global
exclusion. Restore is always explicit and audited.

Until native Apple matching resumes, Spatial Audio curation uses an explicit
manual workaround:

1. Chordrift or the user creates a dedicated Spotify playlist of candidate
   tracks.
2. The user scans its public Spotify URL with
   [Hello Atmos](https://helloatmos.app/spotify/) to identify Apple Music Dolby
   Atmos matches.
3. The filtered set is exported to a specially named Apple Music Spatial Audio
   playlist directly, or copied into a temporary filtered Spotify playlist and
   mirrored with SongShift.

This third-party result is a convenience, not authoritative Chordrift provider
state. It must not silently populate verified Apple identifiers or Spatial
Audio flags in Neon. A future native adapter will match exact recordings,
retain storefront and evidence provenance, and cache Apple's extended
`audioVariants` value `dolby-atmos` so repeated queries are unnecessary.

## v0.0.5 — Embeddings

Build versioned hybrid representations. Use a pretrained music-audio foundation
model (initially MERT, with MuQ evaluated as an alternative) as the reusable
acoustic base whenever Chordrift has lawful access to locally owned, DRM-free
audio. Add semantic context from explicitly semantic legacy playlists, artist
and album relationships, and historical names. Add independently sourced
recording/release language, release country, and artist-area evidence with
source, confidence, and retrieval provenance; never infer origin from Spotify
availability markets. Prefer MusicBrainz for this enrichment and keep unknown
values unknown rather than guessing from titles. Store behavioral preference and
lifecycle signals—plays, recency, completion, skips, provider-curated rotation,
inbox status, and recommendation provenance—separately so unrelated favorites
do not become acoustically similar merely because both are frequently played.
Spotify-only tracks must retain a deterministic personal/metadata fallback
rather than downloading or scraping provider audio.

Chordrift does not train a music foundation model. It performs inference with
pretrained models or imports independently sourced semantic tags for an
identified recording, then caches the result with model/source, version,
confidence, and retrieval time. Before clustering ships, review the then-current
Spotify Platform policy and keep Spotify as the live synchronization and
user-action provider; resolve the artist/title/ISRC identity independently for
model inference and portable enrichment.

Status: complete. Provenance-aware external semantic enrichment is the first
input task for v0.0.6 before cluster generation begins.

## v0.0.6 — Vibe clustering

First enrich canonical recording identities independently from Spotify. Use a
rate-limited, cache-first MusicBrainz adapter with ISRC-first resolution,
conservative ambiguity handling, and raw-response retention. Persist genres,
folksonomy tags, release countries, and release-title language/script with
source, parser version, confidence, and entity provenance. Resolve credited
artists to separately versioned primary-associated-area evidence, retaining
unknown outcomes and never treating the area as birthplace, nationality, or
track language. Add pretrained mood/sound inference without confusing
release-title language with vocal language.

Then create reproducible cluster generations with stable identities,
representatives, statistics, and support for unassigned tracks. Cluster inputs
must identify the exact enrichment and embedding generations they consumed.
After proposed playlists have stable identities, add account-scoped assignment
feedback: reject a track's current vibe, prefer or lock another destination,
and make the next generation honor that decision as an auditable stability
constraint. Preserve the original model score and prior assignment rather than
rewriting history. This is post-generation correction, not a prerequisite for
the initial classification.

The same feedback surface handles initially unassigned tracks: create a stable
manual semantic destination, assign or move a track using its provider ID, or
leave it in an internal needs-review queue. Active decisions replay into later
proposals; changing a decision supersedes rather than erases its audit record.
Needs Review is never published as a provider playlist and does not satisfy
retirement coverage.

## v0.0.7 — Naming and proposed library

Generate names, descriptions, and semantic tags, then expose a complete,
inspectable, non-destructive proposed playlist structure. Require user approval
of generated names and organization, and prove that every track from each
legacy or discovery playlist is represented before proposing its retirement.

Playlist identity must be independent of both temporary cluster labels and
generated display names, and should carry forward through later generations by
membership lineage. Naming uses a strict model-neutral export/import artifact
with provider/model/version/hash provenance. Approval is generation-specific
and is blocked by stale or incomplete naming, unassigned retirement-source
tracks, or incomplete legacy/intake coverage. This milestone never writes to
Spotify.

The preservation universe is account-scoped and durable: latest saved tracks
plus tracks ever retained through semantic-legacy, transport, intake, or
Chordrift-managed playlist membership. Listening history influences ranking
and classification but is not library membership by itself. A proposal is
publishable only when every preserved track has exactly one acceptable
disposition: canonical placement or an explicit reversible exclusion. This
invariant must hold independently for every future connected account.

## v0.0.8 — Full dry-run synchronization

Plan idempotent Spotify diffs without mutating the service. Include discovery
inbox ingestion, cross-playlist duplicate removal, and explicit retirement
plans for legacy and consumed inbox playlists, with track-preservation checks
and no implicit deletions. Show proposed additions to and restorations from
Excluded Tracks separately from provider drift. Provider-neutral plan
structures must allow Apple Music diffs and Spatial Audio variants to be added
later without changing the canonical model.

## v0.0.9 — Spotify apply readiness

Validate approval records, operation ordering, interruption recovery,
rate-limit handling, and convergence checks against Spotify fixtures and
read-only probes. Continue to prohibit remote mutations while proving that an
approved plan can be executed safely and audited completely.

Apply readiness is recorded as an immutable assessment of one exact dry-run.
It validates snapshot freshness, proposal and artwork approval, operation
integrity and destructive gates, the external-cleanup approval, five simulated
resume checkpoints, bounded 429 retry behavior, zero-change replay, and one
explicit read-only Spotify identity/scope probe. A missing probe or stale gate
produces an inspectable blocked assessment; neither state enables writes.

Inventory owned, provider-curated, collaborative-external, and followed-
external playlists as distinct relationships. Add read-only bookmark list and
content inspection, preserve accessible external contents before cleanup, and
show provider-library removal as a separate explicitly approved plan category.
External bookmarks must never be mistaken for owned legacy playlists or
canonical inputs.

The bookmark foundation persists stable records and immutable pull-bound
observations in Neon. Normal pulls detect snapshot-signature changes for
relationships still visible to Spotify; public followed contents remain
metadata-only under Development Mode, while readable collaborative contents
are copied forward without redundant requests. Explicit on-demand refresh now
targets exactly one present or archived bookmark, stores complete and denied
attempts separately from provider-library snapshots, preserves the last
readable contents, and does not increase normal-sync requests. Under Spotify's
current Development Mode, item refresh succeeds only for owned/collaborative
playlists; ordinary followed public playlists remain metadata bookmarks.

External cleanup review is represented by an immutable candidate batch. The
user approves its exact ID after inspecting all owners, playlist IDs,
preservation states, and Spotify signatures. Only a still-current approved
batch may add relationship-only `remove_external_playlist` operations to the
dry-run; changed membership or signatures require new review. v0.0.9 continues
to prohibit execution of those operations.

Generate one simple original cover-art artifact for every approved canonical
playlist before publication. Artwork should be derived from the approved name,
description, and semantic tags; use a restrained shared visual system while
remaining distinct per playlist. Store generator/prompt or template version,
dimensions, media type, and content SHA-256, provide a local contact sheet or
equivalent preview, and require explicit approval. Identical inputs must reuse
identical artifacts. Do not use provider artwork, copyrighted source images, or
personal photos, and do not request Spotify image-upload scope or upload a
cover during this read-only milestone.

The first implementation uses the original **Drift Atlas v1** visual system:
14 local 1254×1254 PNGs, one per stable canonical playlist key, plus a contact
sheet and strict manifest. Import validates complete proposal coverage, names,
dimensions, media types, and SHA-256 values before Neon records an immutable
review batch; approval remains a separate local-only command.

## v0.1.0 — Canonical music library

Synchronize approved canonical playlists from Neon to Spotify, retain
provenance and operation history, and converge to zero changes on repeated
runs. Upload only explicitly approved cover artifacts after the corresponding
playlist exists, with the same interruption and convergence protections as
track operations. Remove legacy or consumed discovery playlists only as
separately approved operations after their replacement playlists are published
and verified. Remove followed/shared external playlists from the user's
provider library only after their bookmark snapshots are retained and the
separate cleanup operations are approved; never mutate the source owner's
playlist. Apple Music publishing and Spatial Audio companions remain a
subsequent provider
milestone unless the deferred provider track is completed earlier.

Status: complete. The v6 planner and v2 readiness probe gate durable phase
executions, per-operation retry history, provider target resolution, exact
retirement approval, post-pull convergence proof, and batched Spotify requests.
The first live migration published 14 canonical playlists with approved covers
and 884 ordered memberships, preserved three intake surfaces, archived external
bookmarks before cleanup, and retired every approved legacy and utility
container. The final imported Spotify surface contains 19 purposeful playlists
with zero duplicate entries and no pending destructive operations.

## v0.1.1 — Explainability and complete visual surfaces

Add one fast track-inspection command that answers whether a song is already in
the current Spotify surface, its approved canonical destination, retained
source-playlist history, listening/lifecycle signals, embedding generation,
cluster score, independent mood/sound facts, and any manual assignment or
exclusion rationale. Title lookup supports artist disambiguation and stable
Spotify IDs.

Extend Drift Atlas artwork to every Chordrift-owned intake, retain pristine
label-free masters for future provider-specific typography, and render exact
labels locally with an approved platform font rather than generated text. Add
`From Prompts` as a fourth intake carrying prompted-discovery provenance. Cover
planning is convergent: an identical content hash already uploaded to the same
stable Spotify playlist is not requested again. A focused
`chordrift artwork update --playlist NAME` command builds an immutable
one-cover plan for a newer approved artifact without admitting membership or
cleanup operations. Spotify folders remain manual presentation state because
the Web API exposes neither folder structure nor folder artwork.

Status: complete and released as v0.1.1.

The complete-library repair is implemented and proven in Neon for the personal
account: 1,715 distinct preserved tracks, 1,715 canonical placements, zero
active exclusions, zero unresolved tracks, and zero conflicting dispositions.
Embedding v4 adds normalized meaningful listening-session co-occurrence so
personal listening context helps singleton tracks without turning every played
track into library membership. Stable extension, centroid, group-consensus,
and reversible manual-assignment paths preserve the 14 approved playlist
identities; evidence did not warrant another canonical playlist in this repair.
The repaired proposal and its 1,715-track Spotify publication are verified.
Drift Atlas v3's 18 larger, lower-anchored covers are also approved, uploaded,
and verified without changing playlist membership.

## v0.1.2 — Listening review and preference learning

- [x] Add a revisioned private user-classification sidecar (`collection`,
  multi-valued `region`, `tradition`, `cohort`, and `language`, plus non-vector
  `notes`). Cohorts express personal composition intent without claiming that
  their tracks sound alike.
- [x] Support immediate one-track corrections and inert CSV export → draft
  import → exact-ID approval for larger regional review passes.
- [x] Feed active explicit facts into a separate higher-weight personalized
  feature namespace without altering pretrained acoustic vectors or public
  metadata.
- [x] Use the approved South Asian classification batch to retire Monsoon
  Cinema into `Dakshina Pulse`, `Uttara Glow`, and the personal `Rasa Archive`,
  while returning globally classified score and pop material to existing
  sound-based destinations. The complete proposal has zero unresolved or
  conflicting tracks; distinct artwork remains the final user-review gate.
- [ ] Add display preferences under
  `$XDG_CONFIG_HOME/chordrift/config.toml` (table width/layout, color policy,
  compact versus detailed inspection, and date formatting). Keep automatic
  terminal-width detection and sensible interactive defaults when unset.
  Deferred to v0.2.0 so it does not block the personal v0.1.2 reconciliation.
- [x] Publish a user-dimension glossary with literal CSV/Excel templates and a
  future account-scoped drag-and-drop token interaction model.
- [ ] Before testing a friend's account, run the two-account isolation and
  provider-boundary audit in
  `docs/design/ACCOUNT_AND_PROVIDER_BOUNDARIES.md`; fix Spotify-specific domain
  leakage before treating the CLI as a reusable product foundation. Deferred
  to v0.2.0 before any second-account or product claim.

Turn ongoing listening corrections into durable, explainable account knowledge
rather than ad hoc Spotify edits. A correction distinguishes four separate
intentions: reject the current destination, prefer or lock an existing
destination, hold the track for review, or exclude the track from active
Chordrift playlists. Every decision retains the prior assignment, model score,
reason, actor, timestamp, and affected provider state; no correction erases the
track, its provenance, or listening history.

Inventory Spotify saved albums as a distinct immutable provider surface before
personal cleanup. Preserve ordered album tracks and account-scoped policy in
Neon, but do not force album-only tracks into normal playlist readiness. Album
cleanup is opt-in and review-gated: each track must already be preserved in
Liked Songs/current playlists or explicitly excluded before the album may be
proposed for unsaving. Product default is preserve; Suhail's personal target is
review-then-unsave and ultimately a playlist-only active library.

Ongoing listening must not require a dedicated review session. Split the
workflow into **capture now** and **reconcile later**. While listening, one
low-friction action records intent and playback continues; naming, cohort
review, artwork, and publication can happen asynchronously. Add three stable
action intents:

- **Refile** — keep the track, but reject its current destination;
- **Review** — keep the track and defer both rejection and destination;
- **Exclude** — propose removing the track from active Chordrift playlists,
  still subject to explicit confirmation and reversible history.

Use one provider-native `Re-evaluate` holding queue rather than parallel
destination routes. While listening, the user adds a misplaced track to
Re-evaluate and removes it from the wrong destination. Chordrift preserves the
entry and rejected source, gives the queue zero preference weight, suppresses
both exclusion and source restoration while it is present, and removes it only
after a newer explicit assignment targets a different approved destination.
Long queues export through the normal classification CSV workflow.

Legacy multi-route surfaces remain immutable history. Exact-confirmed
`reevaluate retire-legacy` requires the replacement queue plus complete
proposal-or-exclusion coverage, changes only Neon, and lets the next reviewed
plan archive the obsolete Spotify containers after publication.

A later review session starts from the currently approved library without
rebuilding or renaming unaffected playlists. The CLI and future UI should
support:

1. Identify a track by provider ID or unambiguous title and artist.
2. Reject its current playlist and record why it feels wrong to the listener.
3. Rank existing destinations with both model evidence and the user's prior
   corrections visible.
4. Move or lock the track to an existing destination, or place it in an
   internal review cohort.
5. Promote a coherent review cohort into a newly named and illustrated
   Chordrift playlist only when it genuinely warrants a new identity.
6. Preview the exact membership/order delta, approve it, publish through the
   existing immutable-plan/readiness gates, pull, and prove convergence.

Track-specific corrections are hard account constraints in later generations:
“not Tidal Hush” must prevent the track from drifting back there. Cultural,
regional, linguistic, soundtrack, instrumentation, and mood facts remain
separate semantic facets. Artist identity alone must not force placement—for
example, an A. R. Rahman recording may be South Asian cinematic, Western
orchestral, ambient, pop, or a mixture. One correction stays local; after
multiple consistent corrections Chordrift may propose a broader account rule
or destination concept, but it must show the evidence and obtain approval
before applying that rule to other tracks.

Treat edits made directly in Spotify as possible feedback rather than either
ignoring them or automatically declaring Neon wrong. Compare each pull with the
last verified managed baseline and stage ambiguous intent for review:

- removal from one managed playlist plus addition to another is a proposed
  move;
- removal alone asks whether the user meant wrong vibe, temporary review, or
  exclusion;
- addition to a managed playlist is a proposed destination preference;
- reordering asks whether the sequence should be locked or remain
  Chordrift-generated.

No ambiguous provider edit is silently learned or reversed. Once confirmed,
the decision is written to Neon and becomes part of future orchestration. This
lets a consumer use Spotify as the familiar editing surface while Chordrift
acts as a preservation-first assistant on top; the dedicated Chordrift review
surface remains the faster, more explainable path for batch corrections.

Maintain the provider-change interpretation matrix in
`docs/design/PLATFORM_INTENT_MODEL.md`. Each personal CLI edge case should
graduate into an explicit product rule: the observed mutation, plausible
intents, confidence boundary, reversible automatic action, and point at which
the user must confirm. Task-oriented personal workflows live under
`docs/how-to/`; the large command catalog is reference material rather than the
primary user journey.

For immediate provider-native capture, adding the playing track to a routing
queue must be enough; the user need not also remove it from the current
canonical playlist. A normal pull records the queue event in Neon. Publication
later adds the verified destination before removing the rejected membership,
so interruption cannot lose the track. A lightweight command or future mobile
shortcut may offer the equivalent `mark current` action, but it must use the
same durable queue model and avoid requiring unsafe interaction while driving
or cycling.

For the personal CLI workflow, add concise `review` commands that clone the
approved generation into a draft, show captured routes and suggestions, record
single or batch corrections, display an exact diff, and approve the draft.
Reuse the existing stable playlist concepts, assignment-revision ledger,
complete-inventory invariant, artwork approval, sync planning, readiness, and
apply machinery rather than creating a second source of truth.

Keep provider and database traffic proportional to observed change. Neon is a
durable cache and change ledger, not a target for wholesale rewrites: unchanged
playlist bodies copy forward by provider snapshot ID; unchanged saved-library
state copies forward after a bounded probe; a changed saved-library snapshot
resolves known provider records in one lookup, rewrites metadata only when its
payload changed, batches snapshot membership, and updates observation times as
one set. Long phases expose progress. Interactive list commands use compact,
colored, width-aware tables; redirected output remains stable plain text for
scripts. Future saved-library pagination should safely reuse an aligned stored
tail when a prefix diff proves that the remaining ordered membership is
unchanged, reducing Spotify requests without weakening complete-inventory
proofs.

Status: complete. The approved South Asian reconciliation, canonical artwork,
Re-evaluate replacement queue, legacy retirement, consumed Inbox cleanup, and
opt-in Liked Songs cleanup have been published and provider-verified. The final
v0.1.2 snapshot converges to a zero-operation plan. Configurable terminal
presentation and the two-account/provider-boundary audit remain intentionally
deferred to v0.2.0.

Saved-album inventory, opt-in Liked Songs consumption, exclusion-aware
readiness/execution/verification, batched changed-surface persistence, and live
zero-plan convergence are implemented. Exact archive-only album-container
retirement retains immutable album and ordered-track history without forcing
album tracks into playlists; review-then-unsave remains the stricter
alternative. The default policies for both albums and Liked Songs remain
preserve.

## v0.2.0 — Recipe foundation and native review UI

Before recipe or UI implementation, complete the database-v2 foundation in
`docs/design/DATABASE_ARCHITECTURE_V2.md`. The v0.1.2 database is logically
healthy but stores raw listening metadata per event and complete provider
membership per routine snapshot. Preserve its verified backup, rehearse a full
restore, separate current provider state from durable intent, normalized
evidence, and rebuildable caches, then migrate through measured invariants.
Database cleanup, schema restructuring, migration/cutover, and code refactoring
are sequential gates; native UI implementation must target the stable v2
bridge rather than legacy tables.

Safe cleanup foundation status: complete. The backup checksum and PostgreSQL
18 restore were verified; logical invariants, physical storage, and protected
versus redundant snapshot classes are repeatable through read-only `db`
reports; and compaction planning cannot mutate a database or contact a
provider. The next sequential gate is implementing and rehearsing the v2 schema
and migrations. Production cutover and deletion remain separately approval-
gated.

Database-v2 schema status: additive foundation complete. Migration 0040 adds
content-addressed provider revisions, one current inventory per account,
compact checkpoint structures, historical provider identities, normalized
listening evidence, and provider-neutral cutover diagnostics. Current-state
backfill and repeated-import revision reuse are proven on PostgreSQL 18, and a
full restored-copy rehearsal preserved the v1 invariant report byte-for-byte.
The next gate migrates normalized evidence and durable snapshot references on a
rehearsal copy, verifies parity, and only then requests separate production
cutover authority.

Database-v2 migration rehearsal status: complete. Migrations 0041 and 0042 add
exact-confirmed normalized-evidence/checkpoint migration plus local dual-write
compatibility. A fresh PostgreSQL 18 clone migrated all 149,314 events and 463
durable audit references with exact invariant parity; 41 referenced snapshots
deduplicated into 24 checkpoints. Independent verification, idempotent replay,
PostgreSQL integration tests, and `pg_amcheck` pass. The read-only cutover plan
is now available, but production apply/read cutover and every legacy deletion
remain separate approval gates. After an approved production observation
window, refactor recipes and provider queries onto v2 before beginning the
native review UI.

Database-v2 production preflight status: complete and read-only. Migration 0043
makes v2 hashes stable across production and rehearsal collations. Production
is healthy on PostgreSQL 18.6 with 39/43 migrations; its invariant report is
byte-identical to the pristine restore, all 17 non-empty playlist hashes match,
and a prospective current-state hash matches the fresh 43-migration rehearsal.
No production write occurred. The next separately approved gate is additive
migrations 0040-0043 only, followed immediately by read-only reports and a stop
to present the actual production data-plan hash. Normalized-evidence apply,
read cutover, observation, and cleanup remain later approval gates.

Database-v2 additive production schema gate status: complete. The explicitly
approved migrations 0040-0043 reached 43/43 on Neon in 3.964 seconds with zero
failures. Post-migration read-only reports preserved the complete legacy
invariant, proved exact current provider parity, and emitted applicable data
plan hash
`a850fb15603f82c934daa127cfb768084938bc8ac601b6f30643ebc3a84e2ae8`.
Normalized evidence and checkpoints remain empty; no data apply, read cutover,
deletion, connection change, or Spotify operation occurred. Exact-confirmed
production data migration is the next separately approved gate.

Database-v2 production data migration status: storage-blocked with a clean
logical rollback. The exact-confirmed plan began under explicit approval but
PostgreSQL returned SQLSTATE `53100`. No v2 evidence, identity, checkpoint, or
receipt row is visible; the complete legacy invariant and plan hash remain
unchanged. Aborted tuples increased physical storage to 514,457,600 database
bytes, including 98,500,608 bytes in the empty normalized-event relation and
4,128,768 bytes in the empty identity relation. Do not retry, vacuum, compact,
or cut over until additional Neon headroom or a separately approved bounded
maintenance strategy is available.

Database-v2 no-cost candidate status: complete and verified. A new isolated
free Neon PostgreSQL 18 project in the same region restored the trusted dump at
249,331,712 bytes, reached 43/43 migrations, and successfully applied exact
data plan
`a850fb15603f82c934daa127cfb768084938bc8ac601b6f30643ebc3a84e2ae8`.
All current-state, event, duration, timestamp, identity, archive, and durable
checkpoint invariants pass; `ready_for_cutover` is true. The dual-storage
candidate is 358,686,720 bytes, below the free 0.5 GB allowance. Its role
password was rotated immediately after a candidate-only URI appeared in a
failed checker diagnostic; no production credential was exposed. Managed Neon
cannot install the superuser-only `amcheck` extension, while the equivalent
local rehearsal already passed it. Production remains configured and logically
unchanged. Connection cutover, observation, cleanup, and old-project deletion
remain separate approval gates.

Database-v2 project connection cutover status: complete and verified. The
private Apogee `CHORDRIFT_DATABASE_URL` value now targets the verified candidate
and retains owner-only file permissions. A fresh Apogee-loaded process proved
43/43 migrations, exact invariant and normalized-evidence parity, 24 resolved
checkpoints, `ready_for_cutover: true`, unchanged cutover hash, and 358,686,720
database bytes. The former project remains intact as rollback protection. This
changes the database project but does not yet refactor individual application
queries from legacy to v2 tables. Observation, read-path refactoring, cleanup,
rollback, and old-project deletion remain separately controlled gates.

Database-v2 v0.1.3 runtime status: implemented and locally verified. Migration
0044 adds stable v2 runtime read surfaces, transient provider-import surfaces,
and exact cleanup receipts. Ordinary application reads no longer depend on
duplicated snapshot bodies; Spotify archives and recent observations write
directly to normalized listening evidence; provider pulls materialize reusable
content revisions and clear import staging before commit. A second fresh
PostgreSQL 18 restore migrated all 149,314 events, applied rehearsal-only cleanup
plan `0688bf0984ea6f6b26cf65ca7ab1c9fcb762601c6a512b204e7a79312830f964`,
and preserved invariant fingerprint
`24f5da45845bb48b3cfeb49cbd09fe371043c7f9544ea38993d3016beaf0d6a3`.
The clean database retains 58 lightweight observations, 22 current playlists,
1,790 ordered memberships, both archive manifests, 24 checkpoints, and every
durable audit reference while shrinking locally to 167,974,591 bytes. Both a
provider-inventory round trip and a normalized archive-import round trip pass
against the post-clean schema. Migration 0044 and destructive cleanup have not
been applied to Neon; each remains a separate exact production gate.

The verified live Neon project keeps stable ID `damp-hall-40280714` and now has
the final display name `Chordrift`; renaming changed neither its connection nor
database contents. The former project remains untouched for rollback.

Build a provider-neutral review UI around the same audited model rather than
moving policy out of the Rust core. It should answer “why is this here?” for
every playlist and track, distinguish canonical collections, intake, generated
experiences, bookmarks, and immutable history, preview organization and
artwork, approve cleanup in bounded batches, explain unknown provenance, and
capture corrections when a track belongs elsewhere.

Implement the provider-neutral recipe domain before building elaborate UI:

- versioned recipe definitions and immutable generation records;
- canonical, intake, and generated-surface roles as distinct concepts;
- provider capability and evidence-availability reporting;
- eligibility, weights, constraints, repetition budgets, and ordering policies;
- per-track inclusion and ordering explanations;
- deterministic preview with no provider writes;
- an initial `New Discoveries + Rediscovery` recipe that can use Like/save time,
  recent observations, and optional extended history;
- a thin native client that can inspect a proposal, adjust a small set of
  meaningful controls, open a track in its provider, and approve through the
  existing immutable execution gates.

The first client presents provider artwork, canonical title and artist, current
destination, listening evidence, and a provider deep link. Double-click opens
the installed provider client; playback and catalog ownership remain with the
provider. The Rust core remains authoritative for identity, recipes,
classification, proposals, history, commands, and diagnostics. A thin native
bridge exposes typed query/command DTOs; provider adapters own OAuth, inventory,
publication, capability reporting, and deep-link construction.

This milestone also owns configurable terminal presentation, the complete
two-account isolation audit, provider-neutral identifiers, and a first-class
Re-evaluate review surface. Do not claim reusable multi-account product support
until that audit passes.

The native app establishes the production operating model. It runs scheduled
work through a quiet background helper or system service without opening
Terminal windows or visible helper shells. It surfaces progress, completion,
actionable failures, cancellation, and recovery in the app or normal OS
notifications. OAuth opens the system browser only for initial or renewed
consent and returns to the app cleanly. Release credentials and tokens live in
1Password and the OS credential store; Apogee may expose approved development
configuration, but the shipped product must not require Apogee. Never place
secrets in config files, logs, shell history, source control, or launch scripts.

Evaluate a dedicated classic Hindi cinema destination after v0.1.2. For now,
misplaced older Hindi songs enter `Re-evaluate` and retain their source and
classification history. A later CSV review should distinguish era, language,
cinema tradition, and listening intent before proposing a poetic Sanskrit-
inspired identity and approved artwork; do not create the playlist merely from
artist identity or a few edge cases.

Regional reconciliation operates over the complete approved library, not only
the playlist where mistakes were noticed. Treat explicit user decisions as
stronger than embedding similarity, require positive evidence before assigning
a tradition, and return globally classified tracks to sound-based destinations
rather than a generic international bucket.

## v0.3.0 — Agentic audit and visual recipe authoring

Turn first-run setup into a read-only audit and editable recommendation plan.
Explain overlap, duplicates, uncertain placement, legacy containers, collection
candidates, available evidence, missing capabilities, and starter recipes.
Present simple recipe philosophies first and reveal detailed controls on demand.
The agent may inspect and propose freely; it obtains separate bounded approval
for publication and destructive cleanup and shows the exact provider diff
before either.

Add visual collection-policy controls, hard versus soft boundaries, generated
playlist schedules, and reproducible previews. Keep the configuration format
authoritative and editable outside the UI.

## v0.4.0 — Learned correction policies

Promote `Re-evaluate` into a complete review experience. Learn only from
explicit approved corrections, distinguish a one-track exception from a
reusable account rule, quantify confidence, and send ambiguous or conflicting
tracks back to review. Allow high-confidence automatic routing only under an
explicit user policy with inspectable history and an immediate override path.

## v0.5.0 — Rolling listening experiences

Add scheduled daily and periodic generation, stable provider playlist identity,
atomic replacement, historical generation comparison, freshness windows,
cross-playlist duplication budgets, and ordering strategies such as energy
arcs, smooth transitions, intentional contrast, and user-defined sections.
Recipes degrade honestly when a provider or account lacks required data.

## Toward v1.0.0 — Shippable product

Use the remaining 0.x releases for additional providers, multi-account proof,
recovery and migration, performance, accessibility, signed installation,
background scheduling, privacy controls, polished onboarding, documentation,
and end-to-end product testing. v1.0.0 means a fully working, installable,
recoverable application—not merely a stable internal schema or architectural
preview.
