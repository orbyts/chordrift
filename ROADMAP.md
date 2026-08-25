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
- [ ] Use the approved South Asian classification batch to split Monsoon Cinema
  into verified North Indian, South Indian, Indian Classical, and any justified
  sound-based international destinations; then coin poetic names and approve
  distinct artwork.
- [ ] Add display preferences under
  `$XDG_CONFIG_HOME/chordrift/config.toml` (table width/layout, color policy,
  compact versus detailed inspection, and date formatting). Keep automatic
  terminal-width detection and sensible interactive defaults when unset.
- [x] Publish a user-dimension glossary with literal CSV/Excel templates and a
  future account-scoped drag-and-drop token interaction model.
- [ ] Before testing a friend's account, run the two-account isolation and
  provider-boundary audit in
  `docs/design/ACCOUNT_AND_PROVIDER_BOUNDARIES.md`; fix Spotify-specific domain
  leakage before treating the CLI as a reusable product foundation.

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

Support optional destination routing queues once the user recognizes a
recurring category, such as `Route — South Indian`, `Route — North Indian`, and
`Route — Decide Later`. These are short-lived action inboxes, not final
listening playlists. Their user-authored descriptions are semantic policy
inputs, never executable instructions. Chordrift consumes a queue only after
its tracks have verified destinations, then clears it for reuse.

The first routing subslice provides durable Neon route identities, generic
create/add/list/inspect commands, per-route label-free and Spotify-labeled
artwork, provider-addition capture during pull, zero signal weight, immutable
publish planning, and post-pull exact-membership verification. Artwork follows
the route meaning rather than a universal template: the first South and North
Indian routes use minimal veena and sarod studies, while Decide Later uses a
minimal junction motif. Full route consumption into existing or newly promoted
canonical destinations remains part of the listening-review reconciliation
slice; cleanup must remain gated on verified destination coverage.

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

Status: in progress; durable routing capture and publication implemented as the
first subslice, with incremental persistence and terminal presentation now in
progress.

Saved-album inventory, opt-in Liked Songs consumption, exclusion-aware
readiness/execution/verification, batched changed-surface persistence, and live
zero-plan convergence are implemented. Exact archive-only album-container
retirement retains immutable album and ordered-track history without forcing
album tracks into playlists; review-then-unsave remains the stricter
alternative. The default policies for both albums and Liked Songs remain
preserve.

## Post-v0.1 product direction — Review UI

If the personal workflow proves useful, build a provider-neutral review UI
around the same audited model rather than moving policy out of the CLI. It
should answer “why is this here?” for every playlist and track, distinguish the
active library from bookmarks and immutable history, preview proposed
organization and artwork, approve cleanup in bounded batches, explain unknown
or inaccessible provenance, and capture corrections when a track belongs in a
different vibe. Product validation should establish whether this library-
entropy problem is shared before commercial scope or multi-user operations are
assumed.
Regional reconciliation must operate over the complete approved library, not
only the playlist where mistakes were noticed. Treat explicit North/South route
decisions as stronger than embedding similarity, require positive style
evidence before using an Indian Classical destination, and return non-South-
Asian tracks to sound-based poetic destinations rather than a generic
"International" bucket. Retire a mixed legacy destination such as Monsoon
Cinema only after every source track has exactly one verified replacement or a
durable exclusion. Clear routing surfaces only after the newer assignment is
published and verified.
