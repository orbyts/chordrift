# How to use Chordrift

This is the canonical user-facing guide to the Chordrift CLI. Update it whenever
a command, option, prerequisite, or workflow changes. If the guide becomes hard
to scan, split it into topic pages under `docs/` and turn this page into the
index.

Chordrift currently reads Spotify state into Neon and analyzes it. It does not
create, edit, reorder, or delete anything in Spotify.

## Everyday workflow

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

Configure a provider-native discovery playlist as an inbox:

```console
$ chordrift playlists configure --name "New Music Inbox" --role inbox
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
