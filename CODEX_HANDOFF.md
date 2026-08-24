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
- Released CLI: `chordrift 0.0.4`
- `main` is the Spotify-focused release line. The Apple foundation is isolated
  on `codex/apple-music` and must not be merged until it can be tested with real
  Apple credentials.

Before editing, inspect `git status --short`, the current branch, this file,
`ROADMAP.md`, and `docs/HOW_TO_CHORDRIFT.md`. Preserve unrelated user changes.

## Durable architecture decisions

- Neon PostgreSQL is the canonical and operational source of truth.
- Provider APIs are adapters. Current Spotify state is pulled into immutable
  snapshots; provider state does not compete with Neon authority.
- Chordrift is read-only against Spotify through v0.0.4. It does not create,
  modify, reorder, or delete remote playlists or tracks.
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
3. Export the Atmos subset to Apple Music, or make a filtered Spotify playlist
   and use Apple Music's built-in **Transfer Music** feature.

Hello Atmos is a third party. Its matches are temporary convenience results,
not verified Neon provider state. Native Chordrift matching must retain exact
recording, storefront, timestamp, and evidence provenance.

Apple privacy exports do not require developer membership. Do not implement a
history parser from assumed examples; inspect the user's actual archive first,
then apply the same immutable, cumulative, PII-excluding principles used for
Spotify.

## Current roadmap and next task

Apple was removed from the critical release path. The next planned milestone is
v0.0.5: versioned personal embeddings from Spotify playlist co-occurrence,
artists, metadata, historical names, and available listening signals. See
`ROADMAP.md` for the remaining sequence through Spotify apply readiness and
v0.1.0.

Do not assume an embedding technique or external model before inspecting the
canonical data distribution and defining reproducibility, versioning, and
evaluation criteria. Preserve provider-policy boundaries: Spotify content is
inventory/provenance and is not training data for a general ML model.

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

Current handoff state: `main` is the intended active branch. The working tree
should be clean after the roadmap and handoff documentation commit.
