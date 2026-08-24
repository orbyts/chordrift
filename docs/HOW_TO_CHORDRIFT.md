# How to use Chordrift

This is the canonical user-facing guide to the Chordrift CLI. Update it whenever
a command, option, prerequisite, or workflow changes. If the guide becomes hard
to scan, split it into topic pages under `docs/` and turn this page into the
index.

Chordrift currently reads Spotify state into Neon and analyzes it. It does not
create, edit, reorder, or delete anything in Spotify.

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

## Installation and help

Show the installed version or discover commands and options:

```console
$ chordrift --version
$ chordrift --help
$ chordrift playlists --help
$ chordrift playlists tracks --help
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
updating the provider inventory. Show the most-listened tracks by total playback
duration:

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

## Playlists

### Which playlist should receive a new song?

The stable user-managed intake names and their meanings are:

| Playlist | Add a track when… | Signal retained by Chordrift |
| --- | --- | --- |
| `Inbox` | You discovered it yourself and currently feel strongly about it. | Explicit recent favorite; elevated intake priority. |
| `From Friends` | A friend explicitly recommended it. | Recommendation provenance. Record the friend/source later when the CLI supports per-entry notes. |
| `Liked from Radio` | You discovered it through radio, autoplay, or a similar platform recommendation. | Provider-assisted discovery, distinct from a direct personal find. |

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

1. `Inbox`, `From Friends`, and `Liked from Radio` as the only user-managed
   intake playlists.
2. Spotify-managed sources such as `On Repeat`, `Daily Mix`, and prompted
   playlists.
3. Multiple Chordrift-managed canonical playlists, each with an approved
   generated vibe name.
4. `Chordrift Spatial Audio` as a temporary companion until native Apple Music
   integration replaces the workaround.

Spotify Liked Songs remains a provider library surface and is not a playlist
retirement candidate.

List known playlists, their current presence, item count, role, drift policy,
and stable Spotify ID:

```console
$ chordrift playlists list --account personal
```

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
$ chordrift clusters generate --account personal --count 12 --min-similarity 0.05
$ chordrift clusters status --account personal
$ chordrift clusters list --account personal
$ chordrift clusters tracks --account personal --cluster vibe-0123456789ab --limit 100
```

Clustering uses deterministic farthest-first initialization and spherical
k-means over cosine similarity. The generation records the exact embedding
generation, algorithm/version, seed, parameters, counts, and an input hash;
identical inputs reuse the existing result. Tracks below the configured
centroid similarity remain explicitly unassigned. Machine labels are temporary
content-derived identifiers, not playlist names, and no Spotify playlist is
created or modified.

After proposed playlists have stable identities, Chordrift will support the
listening correction workflow: reject a track's current assignment, optionally
choose or lock a different destination, then regenerate so the track moves.
That account-specific decision will be an auditable constraint while the
original score and assignment remain preserved.

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
