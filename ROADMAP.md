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

Stable user-managed intake names are `Inbox` for direct personal discoveries,
`From Friends` for explicit recommendations, and `Liked from Radio` for
radio/autoplay discoveries. Spotify manages `On Repeat`, `Daily Mix`, and
prompted playlists. Chordrift-managed outputs receive approved generated vibe
names and must not be edited as intake surfaces. The temporary Atmos workaround
uses `Chordrift Spatial Audio`.

The intended final Spotify surface contains only those three user-managed
intakes, Spotify-managed sources, Chordrift-managed canonical playlists, and
the temporary `Chordrift Spatial Audio` companion. All other user-created
legacy vibe and utility playlists are retirement candidates once their
semantic evidence has been consumed and every track has a published, verified
canonical destination. This explicitly includes `Melodi(es)` and
`Ambient Music Therapy – Indian Lounge - Relaxing Music for your Six Senses`.
Retirement removes the old playlist container, not its tracks from the library.
Spotify Liked Songs is a provider library surface rather than a playlist and is
not part of this retirement plan.

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
are copied forward without redundant requests. Archived on-demand refresh
remains a later v0.0.9 slice.

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
