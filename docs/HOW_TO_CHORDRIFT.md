# How to use Chordrift

This is the canonical user-facing guide to the Chordrift CLI. Update it whenever
a command, option, prerequisite, or workflow changes. If the guide becomes hard
to scan, split it into topic pages under `docs/` and turn this page into the
index.

Chordrift reads Spotify state into Neon and changes Spotify only through an
exact inspected plan, readiness assessment, and explicitly confirmed apply
phase.

## Everyday workflow

Chordrift has three deliberately separate synchronization paths:

| Command | Input | Purpose |
| --- | --- | --- |
| `chordrift sync pull` | Spotify's live Web API plus history already in Neon | Update current playlists/saved tracks and relink already-imported history; never scan local ZIPs. |
| `chordrift history ingest` | Newly downloaded ZIPs in the local account inbox | Add only previously unknown historical events, then retain the ZIPs in the local archive. |
| `chordrift history restore` | ZIPs already retained in the local archive | Rebuild enrichment after database recovery; not part of normal synchronization. |

After changing one or more playlists in Spotify, pull those changes into Neon:

```console
$ chordrift sync pull
```

The default account label is `personal`. The equivalent explicit command is:

```console
$ chordrift sync pull --account personal
```

Chordrift always checks the playlist index, but only downloads track entries
for playlists whose Spotify snapshot changed. It probes the newest saved-track
page and reuses the existing Neon snapshot when the count and leading entries
are unchanged. A successful command ends with `analysis: current` and prints the
same snapshot ID for the import and analysis.

To verify the ordered contents now stored in Neon:

```console
$ chordrift playlists tracks --name "Smooth Morning Coffee (Curated)"
```

Playlist names are matched case-insensitively and must be unambiguous. If two
playlists have the same name, list their IDs and select the intended one:

```console
$ chordrift playlists list
$ chordrift playlists tracks --spotify-id 77ejx8LwlokNcr7L1QH8JN
```

The output retains Spotify order and duplicate entries. Display positions are
one-based, while Neon preserves the original zero-based provider positions.

To answer “do I already have this song, where is it, and why?” in one command:

```console
$ chordrift tracks inspect --name "Do Your Best" --artist "John Maus"
$ chordrift tracks inspect --spotify-id SPOTIFY_TRACK_ID
```

`chordrift tracks inspect` reports current Spotify playlists, the approved
Chordrift destination and position, retained historical source playlists,
saved/rotation/discovery/prompt/intake/recommendation signals, listening counts,
the personal embedding generation and dimensions, cluster cosine similarity,
independently imported mood/genre/sound facts, manual assignment reasons, and
active exclusion state. Exact titles must be unambiguous; add `--artist` or use
the stable Spotify ID when several recordings share a title.

## Installation and help

Show the installed version or discover commands and options:

```console
$ chordrift --version
$ chordrift --help
$ chordrift playlists --help
$ chordrift playlists tracks --help
$ chordrift routes --help
```

Chordrift reads its Neon URL from `CHORDRIFT_DATABASE_URL` and its public
Spotify application ID from `CHORDRIFT_SPOTIFY_CLIENT_ID`. With Apogee, expose
those variables through Apogee rather than editing shell initialization files.

## Spotify data archives

Neon remains Chordrift's operational and authoritative database. Chordrift
keeps downloaded Spotify archives under the Git-ignored `data/` directory only
as immutable recovery inputs. Keep Spotify's original `my_spotify_data.zip`
filename; the folder structure distinguishes the two export types:

```text
data/spotify/personal/inbox/
├── account-data/my_spotify_data.zip
└── extended-streaming-history/my_spotify_data.zip
```

After saving new exports in those locations, run:

```console
$ chordrift history ingest
```

Chordrift recognizes each archive from its contents, imports it idempotently,
and moves it beneath `data/spotify/personal/archive/` using export type, import
date, and full SHA-256 folders. The ZIP itself remains named
`my_spotify_data.zip`, while the hash folder prevents collisions across repeated
requests. Reusing an exact archive or importing a newer archive with overlapping
events does not duplicate listening history. Later exports are treated as
cumulative supplements: Chordrift identifies an event by account, Spotify track
ID, timestamp, milliseconds played, and an occurrence number for truly
identical repeats, then inserts only event identities Neon does not already
know.

Inspect one archive without writing to Neon or moving the file:

```console
$ chordrift history inspect --archive data/spotify/personal/inbox/account-data/my_spotify_data.zip
```

Import a specific archive without applying the inbox/archive organization:

```console
$ chordrift history import --archive /path/to/my_spotify_data.zip
```

Summarize the account's imported listening history:

```console
$ chordrift history summary --account personal
```

Reconcile newly encountered Spotify IDs and rebuild per-track statistics from
the retained raw events:

```console
$ chordrift history refresh --account personal
```

Normal `chordrift sync pull` runs this reconciliation automatically after
updating the provider inventory. It also requests only Recently Played events
after Neon’s durable cursor. API observations retain timestamp, track, and
context but do not invent duration, completion, or skip values Spotify did not
return. A later extended-history archive supersedes overlapping provisional
observations before statistics are rebuilt, preventing double counting.

Request Extended Streaming History approximately once a year as a cumulative
backup and gap repair. Spotify publishes no fixed recurring-request quota; do
not start overlapping requests. Save each delivered ZIP in the same inbox and
run `history ingest`; content hashes and event identities discard known data.

Show the most-listened tracks by total playback duration:

```console
$ chordrift history top --account personal --limit 25
```

Extended streaming history is the authoritative event source because it has
timestamps, exact Spotify track URIs, durations, skips, playback reasons, and
context flags. Account-data exports are registered with safe playlist/library
counts, but their simplified recent plays are not imported because they overlap
extended history. Chordrift does not store archive IP addresses, account profile
details, addresses, payment data, messages, or search text.

If Neon must be rebuilt, first restore the application schema and current
provider inventory, then replay every retained local archive:

```console
$ chordrift db migrate
$ chordrift spotify auth --account personal
$ chordrift sync pull --account personal
$ chordrift history restore --account personal
```

`chordrift history restore` scans the content-addressed archive without moving
or modifying its ZIPs. Imports remain idempotent, so it is also safe to run as a
recovery verification against an already-current database. The unchanged ZIPs
let future Chordrift versions extract additional useful fields without asking
Spotify to generate the export again.

## Database

Check Neon connectivity and migration state without changing the schema:

```console
$ chordrift db status
```

Apply all pending application-owned migrations:

```console
$ chordrift db migrate
```

## Spotify account

Authorize an account through Spotify OAuth with PKCE:

```console
$ chordrift spotify auth --account personal
```

Verify the refresh token stored in macOS Passwords/Keychain:

```console
$ chordrift spotify status --account personal
```

Import a read-only snapshot without refreshing derived analysis:

```console
$ chordrift spotify import --account personal
```

For normal use, prefer `chordrift sync pull`; it performs the import and then
refreshes analysis. Remove the locally stored refresh token without changing
Spotify itself:

```console
$ chordrift spotify logout --account personal
```

The account label is local convenience. Neon also retains Spotify's stable user
identity, and all playlist roles are scoped to that account.

Spotify consent is a single PKCE authorization covering inventory, Recently
Played, reserved top-item access, playlist publication, library cleanup, and artwork.
Normal commands do not rewrite an unchanged Keychain credential. For a friend
test, install one stable build rather than repeatedly running changing debug
binaries; a future consumer build must be signed so macOS recognizes the same
application at each launch.

## Playlists

Generate a labeled cover on demand from a label-free 1254×1254 PNG. The input
is never overwritten, so the pristine master remains available for a later
Apple Music typography variant:

```console
$ chordrift artwork render \
    --background artwork/backgrounds/folder-made-for-suhail.png \
    --title "Made for Suhail" \
    --output artwork/rendered/made-for-suhail.png
```

Spotify folders themselves have no Web API cover endpoint; assign this output
manually in a client surface that supports it. For a playlist, register or use
an approved artwork artifact and run `artwork update --playlist NAME` to create
an auditable one-cover plan.

### Routing playlists: capture a correction while listening

Routes are temporary corrective inboxes, not final listening playlists. A
non-empty route means Chordrift has pending recategorization work. Route
membership carries zero clustering, embedding, rotation, or preference weight.
The steady state is empty: Chordrift first retains every routed track in Neon,
then assigns it to an existing canonical playlist or proposes a genuinely new
poetic playlist with its own name and artwork, publishes and verifies that
destination, and only then clears the route.

Create or update a route in Neon with its own meaning-specific artwork. The
name is normalized to the `Route —` prefix. Keep the label-free master as well
as the deterministically labeled Spotify cover:

```console
$ chordrift routes create \
    --account personal \
    --name "South Indian" \
    --description "Corrective inbox for South Indian recordings that need verified reassignment; not a final listening playlist." \
    --background artwork/routing/route-signals-v1/backgrounds/route-south-indian.png \
    --artwork artwork/routing/route-signals-v1/route-south-indian.png
```

Future routes are not required to reuse the regional instrument treatment.
Generate artwork appropriate to each route's meaning, render its Spotify label
with `chordrift artwork render`, and pass both paths to `routes create`.

Add one or several known tracks from the CLI without contacting Spotify:

```console
$ chordrift routes add \
    --account personal \
    --route "South Indian" \
    --spotify-id 42lDp1YYCiy50UtXUO9FNp \
    --spotify-id 1VdBV90HgsUkjdKo95qnLf \
    --reason "Regional context does not belong in Tidal Hush"
```

While listening on the go, the lowest-friction equivalent is to add the
playing track directly to the appropriate `Route — …` playlist in Spotify.
The next `chordrift sync pull` captures that addition into Neon before any
cleanup. Do not also remove the track from its current canonical playlist; the
later verified recategorization performs the safe move.

Inspect all routes or the durable desired membership of one route:

```console
$ chordrift routes list --account personal
$ chordrift routes tracks --account personal --route "Route — South Indian"
```

Creating or adding through `routes` changes Neon only. Publish route creation,
membership, and artwork through the normal inspected workflow:

```console
$ chordrift sync plan --account personal
$ chordrift sync plan-show --account personal --details
$ chordrift sync apply-preflight --account personal
$ chordrift sync readiness --account personal --probe
$ chordrift sync apply --account personal --assessment ASSESSMENT_ID \
    --phase publish --confirm ASSESSMENT_ID
$ chordrift sync pull --account personal
```

The initial review routes are `Route — South Indian`, `Route — North Indian`,
and `Route — Decide Later`. Spotify folders are visual organization only and
are unavailable through the Web API; move the three published routes into a
folder manually if desired.

### User-created playlists and retirement

The default is **retire none**. A newly encountered user-owned playlist is
classified `user_managed`: Neon retains its identity, membership, order, and
history while Chordrift leaves its name and contents alone. Canonical and
intake playlists have their own protected policies.

Retirement selection changes Neon intent only and makes no Spotify request:

```console
$ chordrift playlists retirement --include "Old Mix" --include "Old Utility"
$ chordrift playlists retirement --all --except "Road Trip in My Order"
$ chordrift playlists retirement --none
```

Even an included playlist is not immediately changed. It must appear in an
exact sync plan, have complete track disposition, pass readiness, receive the
separate exact-plan retirement approval, and then be applied with the
destructive flag. This preserves custom playlists for users who prefer their
own name or exact sequence while allowing an explicitly chosen one-time cleanup.

### External playlist bookmarks

The intended clean account surface does not include playlists owned by friends,
other users, or organizations merely because they were followed, added, or
shared collaboratively. Chordrift will classify these as internal **External
Playlist Bookmarks**, not active library playlists.

Before a future cleanup removes one from Spotify or Apple Music, Chordrift will
retain the provider ID, owner, link, relationship, metadata, and last-known
contents when the provider makes them readable. An inaccessible playlist will
be marked incomplete. The retained bookmark remains queryable in Neon, but it
does not contribute to clustering, signals, canonical playlist counts, or
legacy retirement.

Cleanup means removing the external playlist from your provider library. It
never means deleting or editing your friend's source playlist, and it always
requires separate approval. The Spotify importer stores these records in Neon
during a normal pull. Public followed playlists are metadata-only under
Spotify's current Development Mode limits; externally owned collaborative
contents are retained when Spotify permits access and explicitly marked
inaccessible otherwise. Private, account-specific playlists owned by Spotify
remain active provider-curated signal sources rather than bookmarks.

List both currently followed and archived bookmarks:

```console
$ chordrift bookmarks list --account personal
```

`present` says whether Spotify still reports the relationship. `last_changed`
advances when Spotify's playlist snapshot signature changes, so another
`chordrift sync pull` can reveal an update while the playlist is still visible
to the account. Archived bookmarks remain listed after later cleanup.

Inspect the newest complete contents Spotify allowed Chordrift to retain:

```console
$ chordrift bookmarks tracks --account personal --name "alone in the car"
$ chordrift bookmarks tracks --account personal --spotify-id PLAYLIST_ID
```

Names must be unambiguous; the stable ID form always wins. A metadata-only or
inaccessible bookmark with no older complete snapshot reports that limitation
instead of presenting an empty playlist as authoritative. Explicit refresh of
one bookmark is deliberately separate from normal sync:

```console
$ chordrift bookmarks refresh --account personal --name "Shared playlist"
$ chordrift bookmarks refresh --account personal --spotify-id PLAYLIST_ID
$ chordrift bookmarks tracks --account personal --spotify-id PLAYLIST_ID
```

A practical shared-playlist workflow is:

1. Follow/save the shared playlist in Spotify, then run `chordrift sync pull`
   so Chordrift retains its stable bookmark and ownership metadata.
2. Optionally run `bookmarks refresh` for that one playlist. Spotify currently
   exposes item membership to Web API applications only when the current user
   owns or collaborates on the playlist; an ordinary public shared playlist
   will therefore be recorded honestly as `inaccessible` rather than empty.
3. Listen in Spotify and add only songs you want to keep to `Inbox` (or `From
   Friends` when the recommendation itself is the meaningful signal).
4. Run the normal pull and later Chordrift organization workflow. Bookmark
   contents never become semantic inputs automatically.

Every explicit attempt is retained, including 403/404 outcomes, while the last
readable snapshot remains inspectable. Refreshing one bookmark does not create
a provider-library snapshot, does not affect normal sync's request budget, and
does not modify Spotify.

Before removing any followed/shared relationships, create an immutable review
batch containing every currently present external bookmark:

```console
$ chordrift bookmarks cleanup-plan --account personal
$ chordrift bookmarks cleanup-show --account personal
$ chordrift bookmarks cleanup-show --account personal --batch BATCH_ID
```

Review every row, especially `owner_id`, `content`, `spotify_id`, and the
expected Spotify snapshot signature. Approval is exact and performs no Spotify
write:

```console
$ chordrift bookmarks cleanup-approve --account personal --confirm BATCH_ID
```

After approval, rebuild the dry-run with `chordrift sync plan`. It will contain
one `remove_external_playlist` cleanup operation per approved bookmark and
report them separately as `external_cleanups`. These operations only remove
your follow/library relationship; they cannot edit or delete the source
owner's playlist. They remain deferred and non-executable throughout v0.0.9.
An approval remains usable across identical pulls, but any changed signature,
added bookmark, or missing bookmark requires a new review batch.

### Which playlist should receive a new song?

The stable user-managed intake names and their meanings are:

| Playlist | Add a track when… | Signal retained by Chordrift |
| --- | --- | --- |
| `Inbox` | You discovered it yourself and currently feel strongly about it. | Explicit recent favorite; elevated intake priority. |
| `From Friends` | A friend explicitly recommended it. | Recommendation provenance. Record the friend/source later when the CLI supports per-entry notes. |
| `Liked from Radio` | You discovered it through radio, autoplay, or a similar platform recommendation. | Provider-assisted discovery, distinct from a direct personal find. |
| `From Prompts` | You intentionally kept it from a Spotify prompt-generated playlist. | Prompted-interest provenance, distinct from passive discovery. |

These are temporary intake surfaces. A later approved apply operation may clear
an entry only after the track is present in a published and verified canonical
playlist. Until then, Chordrift only reads them. Do not put a track in more than
one intake merely to increase its weight; use the most specific origin.

Spotify-owned playlists such as `On Repeat`, `Daily Mix`, and prompted
playlists are signal sources that Spotify manages. Never add to, rename, empty,
or recreate them for Chordrift. Chordrift reads their meaning without treating
their tracks as a shared musical vibe.

Chordrift-managed canonical playlists will use approved, generated vibe names.
Those names are deliberately not fixed before clustering. Do not manually add
tracks to those playlists; Chordrift will own their membership, while Spotify
is the only live output provider for the first canonical release. A future
Apple Music adapter will use the same approved names. The temporary Spatial
Audio companion is named `Chordrift Spatial Audio`.

Legacy vibe playlists must remain until the dry-run proves that every track has
an approved canonical destination. After that verification and separate user
approval, all old user-created vibe and utility playlists are intended to be
retired. This includes `Melodi(es)` and `Ambient Music Therapy – Indian Lounge
- Relaxing Music for your Six Senses`, which remain semantic evidence until
retirement. The utility playlists `Two Way Sync`, `My top tracks playlist`, and
`All my saved songs` are not intake names and are also retired from the design.
`Collaboration Jessica` is explicitly ignored and must not contribute
recommendation or similarity evidence. Retirement removes the obsolete
playlist container, not its tracks from the saved library or verified canonical
destinations. Deleting any existing playlist is still a manual Spotify action
in v0.0.5; Chordrift will not do it remotely.

The target Spotify surface is therefore:

1. `Inbox`, `From Friends`, `Liked from Radio`, and `From Prompts` as the only user-managed
   intake playlists.
2. Spotify-managed sources such as `On Repeat`, `Daily Mix`, and prompted
   playlists.
3. Multiple Chordrift-managed canonical playlists, each with an approved
   generated vibe name.
4. `Chordrift Spatial Audio` as a temporary companion until native Apple Music
   integration replaces the workaround.

Spotify Liked Songs remains a provider library surface and is not a playlist
retirement candidate.

List only playlists present in Spotify's latest successfully imported snapshot,
using their current Spotify names, item count, role, drift policy, and stable
Spotify ID:

```console
$ chordrift playlists list --account personal
```

Renames replace the active name on the next pull, and removed playlists vanish
from this current-state list. Neon still retains immutable older snapshots for
sync provenance and recovery, but historical names never appear as current
playlists. Proposed Chordrift names live separately until an approved publish.

List the latest imported contents of one playlist:

```console
$ chordrift playlists tracks --name "Smooth Morning Coffee (Curated)"
$ chordrift playlists tracks --spotify-id 77ejx8LwlokNcr7L1QH8JN
```

Configure a personal discovery playlist as an inbox:

```console
$ chordrift playlists configure --name "Inbox" --role inbox
```

Configure a future Chordrift-owned output playlist:

```console
$ chordrift playlists configure --spotify-id SPOTIFY_ID --role managed
```

Roles are:

- `observed`: Spotify is the current authority and Chordrift mirrors it.
- `inbox`: a provider-native discovery surface intended for later consumption.
- `managed`: an approved Chordrift output whose desired state will live in Neon.

Default drift policies are `provider-wins` for observed and inbox playlists and
`neon-wins` for managed playlists. Override one explicitly when needed:

```console
$ chordrift playlists configure --name "Review Together" --role managed --drift-policy manual
```

The available policies are `provider-wins`, `neon-wins`, and `manual`. They are
durable metadata in the current pull-only release; remote repair will arrive in
the later dry-run/apply workflow.

## Embeddings

Chordrift's target representation is hybrid:

1. A reusable, music-foundation embedding such as MERT describes the recording's
   acoustic character when Chordrift has lawful access to an audio file.
2. An account-scoped semantic component describes explicitly semantic legacy
   playlist co-membership, artists, albums, and historical playlist-name tokens.
3. Independently sourced release language/country and artist-area metadata can
   enrich similarity when each value retains its source and confidence.
4. Listening, rotation, intake, and recommendation evidence remains a separate
   versioned signal generation used for composition and ordering, not musical
   similarity.

Spotify track metadata does not contain the waveform required by an acoustic
foundation model. Until a track can be matched to locally owned, DRM-free
audio, Chordrift generates a deterministic semantic metadata fallback. The schema
keeps canonical acoustic embeddings separate from account-scoped generations,
so adding MERT later does not invalidate the provider inventory or history.
Spotify does not provide authoritative track language or recording origin;
availability markets describe where a recording can be played. Chordrift will
enrich those fields from independently licensed sources such as MusicBrainz,
and will preserve unknowns rather than infer them from a title. Spotify remains
the synchronization/evidence adapter, subject to its current Platform policy.
Chordrift is not training a foundation model: it resolves a recording identity,
runs a versioned pretrained model or imports independently sourced tags, and
caches the inference with provenance and confidence.

Audit semantic source coverage and playlist weights:

```console
$ chordrift embeddings audit --account personal
```

Configure each playlist's evidence class without changing it in Spotify. For
example, exclude a catch-all utility playlist:

```console
$ chordrift playlists signals --name "All my saved songs" --class ignored
```

Treat Spotify On Repeat as current rotation evidence without implying that all
of its tracks share a vibe:

```console
$ chordrift playlists signals --name "On Repeat" --class provider-curated --behavior rotation
```

Configure a friend-recommendation intake. Intake clearing remains disabled
until the later apply workflow can verify canonical placement:

```console
$ chordrift playlists signals --name "From Friends" --class intake --behavior recommendation
```

Evidence classes are `semantic-legacy`, `provider-curated`, `intake`,
`canonical`, `transport`, and `ignored`. Semantic weights from `0` through `10`
are accepted only for `semantic-legacy`; the default is `1`.

Generate or reuse the deterministic fallback generation:

```console
$ chordrift embeddings generate --account personal
$ chordrift embeddings status --account personal
```

The default fallback uses 1,024 signed-hash dimensions. The earlier 128-slot
diagnostic produced obvious feature collisions in this library; dimensions,
model version, and seed remain recorded so generations are reproducible.

Generate listening and lifecycle evidence independently:

```console
$ chordrift signals generate --account personal
$ chordrift signals status --account personal
```

The signal generation retains meaningful play count, one-year exponential
recency, completion and non-skip ratios, saved state, configured provider
rotation, intake membership, and recommendation provenance. These values do not
enter nearest-neighbor similarity.

Inspect nearest neighbors before allowing a generation to feed clustering:

```console
$ chordrift embeddings neighbors --name "TRACK TITLE" --limit 10
$ chordrift embeddings neighbors --spotify-id SPOTIFY_TRACK_ID --limit 10
```

Every generation records its model, implementation version, dimensions, seed,
parameters, source snapshot, and content hash. Regenerating identical inputs
reuses the existing immutable generation.

The current fallback generation also consumes versioned MusicBrainz semantic
facts, imported model facts, and any lawful imported acoustic embeddings.
Imported acoustic vectors are L2-normalized and projected deterministically
into the common feature space; model identity and version are part of the
feature key. Behavioral signals remain outside similarity.

## Vibe clusters

Generate a non-destructive diagnostic structure from the latest immutable
embedding generation:

```console
$ chordrift clusters generate --account personal --count 12 --min-similarity 0.05 --min-cluster-size 10
$ chordrift clusters status --account personal
$ chordrift clusters list --account personal
$ chordrift clusters tracks --account personal --cluster vibe-0123456789ab --limit 100
```

Clustering trains deterministic farthest-first spherical k-means centroids on
tracks with genuine semantic seed evidence, then considers all embedded tracks
for assignment by cosine similarity. The generation records the exact embedding
generation, algorithm/version, seed, parameters, counts, and an input hash;
identical inputs reuse the existing result. Tracks below the configured
centroid similarity or in undersized groups remain explicitly unassigned.
Machine labels are temporary
content-derived identifiers, not playlist names, and no Spotify playlist is
created or modified.

After proposed playlists have stable identities, Chordrift will support the
listening correction workflow: reject a track's current assignment, optionally
choose or lock a different destination, then regenerate so the track moves.
That account-specific decision will be an auditable constraint while the
original score and assignment remain preserved.

## Proposed playlist library

Build an immutable, non-destructive proposal from the latest cluster
generation, then inspect its stable playlist identities and tracks:

```console
$ chordrift proposals generate --account personal
$ chordrift proposals status --account personal
$ chordrift proposals list --account personal
$ chordrift proposals tracks --account personal --playlist playlist-0123456789ab --limit 100
$ chordrift proposals coverage --account personal
$ chordrift proposals missing --account personal --limit 100
$ chordrift proposals inventory --account personal
$ chordrift proposals unresolved --account personal --limit 100
$ chordrift proposals placement-audit --account personal
$ chordrift proposals extend --account personal --min-similarity 0.05
$ chordrift proposals group-tracks --account personal --cluster vibe-EXAMPLE --limit 50
$ chordrift proposals consensus-assign --account personal \
    --min-dominance 0.55 --min-evidence 10
$ chordrift proposals centroid-assign --account personal --min-similarity 0.05
```

The proposal copies cluster membership into canonical Neon rows without
creating, renaming, clearing, or deleting any Spotify playlist. A stable
`playlist-*` key is separate from both a cluster's temporary machine label and
its user-facing name. When a later cluster generation overlaps an earlier
playlist by at least half of the smaller membership, Chordrift carries that
stable identity forward. This gives future manual corrections and provider
synchronization a durable target.

`proposals inventory` is the complete preservation proof. Its deduplicated
`complete_inventory` row includes current saved tracks plus tracks ever seen in
the account's semantic-legacy, transport, intake, or Chordrift-managed
playlists. Each track must be placed in the proposal or have an active,
reversible exclusion. `proposals unresolved` lists the tracks with neither
disposition. Listening history and provider-curated playlists enrich ranking
and classification, but listening alone does not turn every casually played
track into library inventory. External bookmarks and explicitly ignored
sources are also outside this universe.

`proposals placement-audit` keeps the approved Chordrift playlist identities
stable and compares each embedded unresolved track with their centroids. Strong
and usable fits can be appended to an existing destination; weak fits remain a
separate review population so a coherent new vibe can become a newly named and
illustrated Chordrift playlist instead of being forced arbitrarily.

After reviewing that split, `proposals extend` clones the latest approved
playlist structure into a new immutable proposal and appends only tracks at or
above the chosen existing-destination similarity. It preserves every concept,
name, description, tag set, and existing membership; it never edits the
approved generation or Spotify. Lower-fit and unembedded tracks remain visible
and continue to block approval.

Use `proposals group-tracks` to review only the still-unresolved members of a
weak-fit analytical group before deciding whether it merits a new managed
playlist.

`proposals consensus-assign` handles tracks with weak individual centroid fit
when their analytical group has a well-supported existing destination. It
requires both a minimum known-member count and dominant share, records the
group generation, score, counts, and threshold in each assignment, and changes
only the editable Neon proposal.

`proposals centroid-assign` performs the complementary direct-fit step against
the current editable playlist centroids. It records the exact embedding
generation, similarity, and threshold and leaves lower-confidence tracks
unresolved.

The older `proposals coverage` and `proposals missing` views remain useful for
inspecting current semantic-legacy and intake retirement sources individually.
They are narrower diagnostics; proposal approval and publishing use the
complete preserved-library invariant.

`proposals missing` lists every uncovered track once, along with its Spotify
track ID and all current retirement-source playlists containing it. Use the
stable Spotify ID when titles are ambiguous.

When automatic evidence is weak, create a stable manual semantic destination:

```console
$ chordrift proposals category-create --account personal \
    --name "Open-Sky Anthems" \
    --description "Earnest alternative and pop-rock songs with widescreen choruses." \
    --tag alternative --tag pop-rock --tag anthemic
```

Assign an unassigned track—or correct an existing placement—using exact stable
identities:

```console
$ chordrift proposals assign --account personal \
    --spotify-id SPOTIFY_TRACK_ID --spotify-id ANOTHER_TRACK_ID \
    --playlist playlist-0123456789ab \
    --reason "Anthemic alternative-rock fit"
```

Repeat `--spotify-id` to assign several reviewed tracks through one database
session. Chordrift still records a separate reversible decision for each track.

Changing the destination later supersedes the prior decision without deleting
its audit record. If no destination feels honest, return the track to the
internal review queue:

```console
$ chordrift proposals review --account personal \
    --spotify-id SPOTIFY_TRACK_ID \
    --reason "Current category does not match this recording"
```

Manual decisions are account-scoped and replay into later proposal generations.
They alter Neon proposal membership only; none of these commands writes to
Spotify. The internal review queue is deliberately not a Spotify playlist and
continues to block retirement coverage until each source track is assigned or
otherwise explicitly resolved.

Naming is a model-neutral artifact boundary. Export a privacy-minimized context
containing stable keys and representative title/artist samples:

```console
$ chordrift proposals naming-export --account personal --file naming-context.json
$ chordrift proposals naming-import --account personal --file naming-results.json
```

The result format is defined by `docs/playlist-naming-v1.schema.json`. It must
target the exact proposal generation and exported context SHA-256, include one
unique name/description/tag set for every stable key, and identify the naming
provider, model, and model/prompt revision. Chordrift retains every revision
and selects the latest import; it rejects unknown fields, reserved intake names,
duplicate names, missing playlists, and stale contexts.

Approval is deliberately explicit and succeeds only after all playlists are
named and retirement coverage is complete:

```console
$ chordrift proposals approve --account personal --confirm PROPOSAL_GENERATION_UUID
```

Approval records the account-owner decision in Neon. It still performs no
Spotify writes; publishing belongs to the later dry-run/apply milestones.

Every approved canonical playlist and Chordrift intake has original Drift Atlas
artwork. v3 preserves the approved backgrounds and adds four intake
backgrounds; exact names are overlaid locally with Helvetica Neue Bold rather
than generated as image text. Label-free masters live under
`artwork/canonical/drift-atlas-v3/backgrounds` so Apple Music can later render
the same artwork with provider-appropriate typography.

The v3 files live under `artwork/canonical/drift-atlas-v3`. Validate the strict
manifest, inspect its contact sheet and hashes, and approve the
exact immutable batch:

```console
$ chordrift db migrate
$ chordrift artwork import --account personal \
    --manifest artwork/canonical/drift-atlas-v3/manifest.json
$ chordrift artwork status --account personal
$ chordrift artwork list --account personal
$ chordrift artwork approve --account personal --confirm ARTWORK_BATCH_UUID
```

`artwork import` verifies the proposal identity, complete playlist coverage,
approved names, every PNG's media type and dimensions, and every content hash.
An identical manifest reuses the existing batch. A changed candidate creates a
new pending review and supersedes any older pending batch. Approval records the
decision in Neon but reports `spotify_writes: disabled`; it neither requests an
image-upload scope nor contacts Spotify.

After importing and approving a newer complete artwork batch, build a focused
one-cover plan by playlist name or stable key:

```console
$ chordrift artwork update --account personal --playlist "Inbox"
$ chordrift sync apply-preflight --account personal --plan PLAN_UUID
$ chordrift sync readiness --account personal --plan PLAN_UUID --probe
$ chordrift sync apply --account personal \
    --assessment ASSESSMENT_UUID --phase publish --confirm ASSESSMENT_UUID
$ chordrift sync pull --account personal
```

`artwork update` is a planner, not a direct provider write. It refuses unknown
or ambiguous selectors, unresolved Spotify targets, and a cover whose latest
approved content hash has already been uploaded to that exact playlist. The
resulting immutable plan contains one `upload_artwork` operation and no playlist
membership or cleanup operations.

Spotify playlist folders are presentation-only client state: Spotify does not
return or create folders through the Web API and exposes no custom folder-cover
surface. Chordrift can manage covers for individual owned playlists, but folder
creation, naming, and placement remain manual and invisible to synchronization.

When exercising unreleased source from a machine-wide shared Cargo target,
isolate the branch build so another clone of the same crate/version cannot
overwrite the executable:

```console
$ cargo run --target-dir target -- proposals status --account personal
```

This is only development hygiene. After a release is installed, use the normal
`chordrift proposals ...` commands.

## Dry-run synchronization plans

Build an immutable plan from the latest approved proposal and the latest
imported Spotify snapshot:

```console
$ chordrift sync plan --account personal
```

To bind planning to a particular approval rather than implicitly selecting the
latest one:

```console
$ chordrift sync plan --account personal \
    --proposal PROPOSAL_GENERATION_UUID
```

The command reads Spotify state already stored in Neon and does not contact or
mutate Spotify. Identical proposal, snapshot, exclusions, and policies reuse
the same plan ID and input hash. A new pull produces a new source snapshot and
therefore a new plan, even when the visible diff happens to look the same.

Inspect the newest plan summary, or print its exact operations:

```console
$ chordrift sync plan-show --account personal
$ chordrift sync plan-show --account personal --details
$ chordrift sync plan-show --account personal --plan PLAN_UUID --details
```

Operations run through ordered safety phases: publish approved destinations,
reconcile managed drift, consume eligible inbox entries, then retire legacy
containers. Inbox removals and legacy retirement remain deferred until every
destination has been published and verified. Retirement also requires a
separate future approval; a dry-run plan is never permission to delete.

The publish phase also proposes any missing stable intake containers named
`Inbox`, `From Friends`, `Liked from Radio`, and `From Prompts`. Existing intake containers are
reused; their tracks are never mixed into a new duplicate container.

Track additions, explicit Excluded Tracks restorations, provider drift, and
new exclusions inferred from a verified managed baseline are reported
separately. A missing expected managed track proposes an internal exclusion
rather than an automatic re-add; an unexpected extra track is provider drift
and does not create an exclusion. Consumed-inbox removals are also distinct.
Provider-curated, transport, ignored, followed, and unmanaged playlists never
become mutation targets.
`snapshot_current: false` means a later pull superseded the plan's observed
state, so generate a new plan rather than relying on the stale one.

## Apply-readiness validation

Before applying any write phase, assess the newest immutable plan and perform
the single-request authenticated identity and scope probe:

```console
$ chordrift sync readiness --account personal --probe
$ chordrift sync readiness-show --account personal
$ chordrift sync readiness-show --account personal \
    --assessment ASSESSMENT_UUID
```

The assessment is stored immutably in Neon. It verifies the current snapshot,
approved proposal and complete approved artwork, ordered and uniquely keyed
operations, destructive-operation gates, the approved external-cleanup batch,
five simulated interruption/resume points, bounded Spotify 429 handling, and
zero changes on an idempotent operation replay. The provider probe refreshes
the existing credential and calls Spotify's current-user endpoint only. For
v0.1.0 it requires the read scopes plus `playlist-modify-private`,
`playlist-modify-public`, `user-library-modify`, and `ugc-image-upload`. Re-run
`chordrift spotify auth --account personal` once to approve those scopes; the
refresh token remains in Passwords/Keychain.

Running `sync readiness` without `--probe` is useful as an offline diagnostic,
but its provider-probe check remains blocked and the overall result is not
ready. Readiness itself never writes to Spotify.

## Applying an approved plan

v0.1.1 executes one safety phase at a time. Every execution requires a current
v8 plan, a ready assessment, and the assessment UUID repeated exactly. A
successful phase stops at `awaiting_pull`; always pull and verify before
planning or applying another phase.

Publish the canonical playlists, any missing intake containers, their ordered
tracks, and only the approved Drift Atlas covers:

```console
$ chordrift db migrate
$ chordrift spotify auth --account personal
$ chordrift sync pull --account personal
$ chordrift sync plan --account personal
$ chordrift sync plan-show --account personal --details
$ chordrift sync apply-preflight --account personal --plan PLAN_UUID
$ chordrift sync readiness --account personal --probe
$ chordrift sync apply --account personal \
    --assessment ASSESSMENT_UUID --phase publish --confirm ASSESSMENT_UUID
$ chordrift sync apply-show --account personal
$ chordrift sync pull --account personal
```

Playlist item additions are batched in groups of 100. On resume, Chordrift
reads each target playlist once, records already-present tracks as successful,
and adds only missing tracks. Repeating the exact apply command resumes its
durable run rather than creating a second execution. Approved PNG covers are
locally converted to Spotify-compatible JPEG payloads no larger than 256 KB.
`sync apply-preflight` performs that hash, decode, and conversion check for every
approved cover and prints the publish request estimate without contacting Spotify.

After the pull reports `verified_apply_runs`, create and assess a new plan.
Only then may intake or external-bookmark cleanup run, with an additional
destructive acknowledgement:

```console
$ chordrift sync apply --account personal \
    --assessment ASSESSMENT_UUID --phase cleanup \
    --confirm ASSESSMENT_UUID --allow-destructive
```

Legacy retirement has one more durable approval boundary. Inspect every
retirement in `sync plan-show --details`, approve that exact plan, and then
execute its retirement phase:

```console
$ chordrift sync retirement-approve --account personal \
    --plan PLAN_UUID --confirm PLAN_UUID
$ chordrift sync apply --account personal \
    --assessment ASSESSMENT_UUID --phase retirement \
    --confirm ASSESSMENT_UUID --allow-destructive
```

Cleanup and retirement are blocked unless every canonical destination matches
the approved ordered membership in the latest imported Spotify snapshot.
Removing an owned playlist means removing its library relationship—Spotify has
no separate playlist-deletion operation—and never deletes an externally owned
source playlist.

## Semantic enrichment

MusicBrainz enrichment is independent from Spotify synchronization. It resolves
canonical recordings by ISRC, conservatively stages ambiguous identifiers, and
caches the complete JSON lookup in Neon before deriving facts. A normal run
processes only tracks without a current parser result:

```console
$ chordrift enrich musicbrainz --account personal --limit 25
$ chordrift enrich status --account personal
```

MusicBrainz asks clients to average no more than one request per second, so the
default batch is intentionally bounded. Resolution uses one cached ISRC lookup,
then one cached recording-detail lookup only after a conservative match. Shared
ISRCs and recording IDs do not cause repeated requests. Reprocess settled tracks
after a parser change without forcing a redownload:

Pending batches prioritize intake, current rotation, saved state, and meaningful
plays so the most useful library surface is enriched first. This changes request
order only; behavioral priority never becomes musical similarity.

```console
$ chordrift enrich musicbrainz --account personal --limit 25 --refresh
```

The first adapter retains recording genres and folksonomy tags, release
countries, and release-title language/script metadata. Release-title language
is not assumed to be the language being sung. Each fact records its MusicBrainz
entity, parser version, confidence, weight, and observation time.

After recording matches exist, resolve their credited artists to MusicBrainz's
primary associated area in another bounded pass:

```console
$ chordrift enrich artists --account personal --limit 25
```

Artist requests use the same durable raw-response cache, so an artist shared by
many tracks is downloaded once. Track-to-artist resolution is independently
versioned and records resolved, unknown, and error outcomes; a missing area
remains unknown, while a transient error becomes eligible for a bounded retry
after its cache delay. MusicBrainz defines this value as the area with which
the artist is primarily identified. Chordrift does not relabel it as
birthplace, formation country, nationality, recording location, or a track's
language. Pretrained mood/sound inference is a later v0.0.6 step.

### Pretrained audio-model artifacts

Chordrift never downloads Spotify audio for inference. A separate local runner
may analyze audio that you own or are otherwise authorized to process, then
emit the strict, path-free JSON format in
`docs/model-inference-v1.schema.json`. Import an artifact with:

```console
$ chordrift enrich model-import --account personal --file inference.json
$ chordrift enrich model-status --account personal
```

The manifest pins the model name, exact version/revision, model license, input
audio SHA-256, sample rate, aggregation method, inference time, embeddings, and
optional genre/mood/sound scores. It contains neither audio nor filesystem
paths. Chordrift rejects provider-audio claims, malformed hashes, non-finite or
oversized vectors, duplicate tracks/facts, unknown fields, and tracks outside
the selected account. Importing identical bytes is idempotent.

The first intended personal-audio runner may evaluate MERT and MuQ-MuLan as
foundation embeddings and Essentia classifiers for explicit mood/sound facts.
The runner and weights stay optional: these models require real audio, and the
currently evaluated weights carry non-commercial terms. Tracks without lawful
audio remain explicitly without an acoustic embedding and continue through the
metadata, semantic-playlist, relationship, and manual-feedback fallbacks.

## Excluded tracks

The planned apply workflow will expose an internal **Excluded Tracks** view,
not a Spotify playlist. After a Chordrift-managed playlist has been published
and verified, removing one of its tracks in Spotify will record a reversible
account-level exclusion during the next pull. Chordrift retains the track,
history, source provider, removal time, and previous assignment, but will not
place it in newly generated playlists until explicitly restored. Removing a
track from a provider-curated, intake, transport, or legacy playlist does not
mean the same thing and will not create an exclusion.

This distinction relies on an immutable successful-verification baseline. A
track missing before a managed playlist has ever matched its approved state is
not enough evidence of an intentional removal. Active exclusions are durable
track dispositions, so legacy retirement and inbox cleanup may preserve a
track through either a verified canonical destination or that explicit
exclusion—never by silently dropping it.

## Analysis

Show aggregate statistics for the current analyzed snapshot:

```console
$ chordrift analyze summary --account personal
```

Refresh analysis from the latest imported snapshot without contacting Spotify:

```console
$ chordrift analyze refresh --account personal
```

List canonical tracks found in multiple playlists:

```console
$ chordrift analyze overlap --account personal --limit 25
```

List repeated canonical tracks within the same playlist:

```console
$ chordrift analyze duplicates --account personal --limit 25
```

If analysis is stale because `spotify import` was used directly, run
`chordrift analyze refresh` or the normal `chordrift sync pull` workflow.

## Before returning to development

Run a pull and confirm the final report says `analysis: current`:

```console
$ chordrift sync pull
$ chordrift playlists tracks --name "Smooth Morning Coffee (Curated)"
$ chordrift analyze summary
$ chordrift history summary
```

That establishes a fresh immutable provider snapshot and matching derived
analysis in Neon. A later run can safely distinguish subsequent Spotify changes
from the clean baseline.
