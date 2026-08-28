# How to use Chordrift

This is the user-facing entry point for Chordrift. Start with the task you want
to accomplish; use the comprehensive [CLI command reference](reference/CLI_COMMANDS.md)
only when you need every option or an internal diagnostic command.

Chordrift treats Neon as the durable ledger and Spotify as the familiar
listening surface. A normal pull observes Spotify changes, but ambiguous intent
is staged for inspection rather than silently guessed.

## Everyday actions

| I want to… | Guide | Spotify action |
| --- | --- | --- |
| Add or discover a song | [Add songs and preserve discovery context](how-to/ADDING_AND_DISCOVERY.md) | Like/Save it; use a named intake only for a richer signal. |
| Review a mixed intake batch safely | [Add songs and preserve discovery context](how-to/ADDING_AND_DISCOVERY.md) | The intake wizard pulls first, isolates pending exclusions, then reviews Liked Songs and named intakes. |
| Inventory or retire saved albums | [Saved albums and album cleanup](how-to/SAVED_ALBUMS.md) | Archive-only retirement keeps immutable album and track history. |
| Stop hearing a song | [Delete or exclude a track safely](how-to/DELETING_AND_EXCLUDING.md) | Remove it from its verified Chordrift playlist, then reconcile. |
| Keep a song but reject its current vibe | [Re-evaluate and reclassify a track](how-to/ROUTING_AND_RECLASSIFYING.md) | Move it to `Re-evaluate` and remove the wrong destination. |
| Add private region, tradition, language, or cohort facts | [Classify tracks with user dimensions](how-to/CLASSIFICATION_DIMENSIONS.md) | Review one track/a small group directly, or approve a CSV batch. |
| Bring Neon up to date | [Synchronize and prove convergence](how-to/SYNC_AND_CONVERGENCE.md) | Run a pull after provider changes. |
| Understand how a future product infers intent | [Platform interaction model](design/PLATFORM_INTENT_MODEL.md) | Keep using Spotify; Chordrift interprets bounded changes. |
| Understand account isolation and provider-neutrality work | [Account isolation and provider boundaries](design/ACCOUNT_AND_PROVIDER_BOUNDARIES.md) | Current personal facts are account-scoped; a full adapter audit precedes another live provider. |

## The short everyday loop

After making a change in Spotify:

```console
$ chordrift sync pull --account personal
```

The v0.1.4 output groups the result into provider, current-library, and
listening-evidence tables and reports elapsed time for each phase. The provider
row also reports how many Spotify API requests the pull used. An unchanged
library is reused in Neon; only new Recently Played observations update
listening evidence and the affected per-track statistics. These timings are the
first diagnostic to share when a routine pull feels slow.

All interactive v0.1.4 commands use the same compact table, color, spacing, and
workflow-progress language. Redirecting stdout preserves the stable plain
key/value and tabular form for scripts. Progress and diagnostic events remain on
stderr, coordinated with one active progress bar so concurrent provider checks
do not overwrite each other.

Then inspect one track when you want to understand its current state:

```console
$ chordrift tracks inspect --name "Do Your Best" --artist "John Maus"
$ chordrift tracks inspect --spotify-id SPOTIFY_TRACK_ID
```

If the change requires a Chordrift write or durable interpretation, create and
inspect a plan:

```console
$ chordrift sync plan --account personal
$ chordrift sync plan-show --account personal --details
```

Never apply a plan merely because it exists. Confirm that its operations match
your intent, then follow the readiness and apply sequence in
[Synchronize and prove convergence](how-to/SYNC_AND_CONVERGENCE.md).

For the complete terminal workflow, the repository includes an operator-only
wrapper that uses the installed `chordrift` binary:

```console
$ scripts/chordrift-workflow.sh --account personal
```

It pulls, creates and displays a plan, runs readiness, asks you to type the
exact assessment UUID, applies one publish or reconcile phase, pulls again,
shows the receipt, and creates the convergence plan. It refuses stale,
multi-phase, cleanup, and retirement plans. Use `--skip-initial-pull` only when
you have just completed and inspected a pull. `CHORDRIFT_BIN` may point to an
alternate installed executable; the wrapper never invokes `cargo run`.

For the complete common intake workflow, including a mandatory fresh provider
pull, reversible exclusion intent, deferred routine duplicate convergence,
exact intake audit, reviewed existing-playlist suggestions, verified
publication, and separately confirmed intake cleanup, use:

```console
$ scripts/chordrift-intake-wizard.sh --account personal
```

The wizard never queries Spotify or Neon directly. Its classification comes
from the read-only Rust `chordrift intake audit` boundary over the exact current
provider snapshot plus durable Chordrift intent, exclusions, and listening
evidence. It stops instead of mixing new-playlist, new-artwork, retirement, or
unrelated unresolved work into the intake batch.

## Semantic playlists used for capture

Use these playlists to tell Chordrift why a newly encountered track matters:

| Playlist | Meaning |
| --- | --- |
| `Inbox` | Strong recent personal discovery. |
| `From Friends` | Explicit recommendation from someone you know. |
| `Liked from Radio` | Discovery from radio or autoplay. |
| `From Prompts` | Discovery from a Spotify prompt-generated playlist. |

Use `Re-evaluate` differently: it means “keep this track, but its current
Chordrift destination needs correction.” Move the track into Re-evaluate and
remove it from the wrong destination. Chordrift retains the event and will not
restore the rejected membership while the track remains in the queue.

Spotify's Like button is the primary lightweight intake action. It means
“keep and classify.” `Inbox` is the stronger high-interest variant; the other
named intakes carry source provenance. For Suhail's opt-in policy, Liked Songs
is cleared only after verified Chordrift placement, just like an intake queue.

Future recipe playlists are a separate layer from both intake and canonical
collections. They will turn recent discoveries, rotation, rediscovery,
favorites, and explicit collections into renewable listening experiences while
preserving why each track belongs in the library. The same engine will support
automatic presets and detailed user controls; see the roadmap for the v0.2.0
foundation.

## Important distinctions

- Removing a track from a verified Chordrift playlist can stage an exclusion.
- Removing it only from Liked Songs means “unsave,” not necessarily “forget.”
- Adding it to Re-evaluate means “keep and reclassify,” not “exclude.”
- A track is eligible for inferred exclusion only when it is absent from both
  its verified destination and Re-evaluate.
- Removing it from a protected user-managed playlist changes that playlist but
  does not automatically teach a global preference.
- Exclusion removes a track from active Chordrift listening surfaces; it does
  not erase listening history, provenance, or recoverability from Neon.
- Known provider-unavailable exceptions can use exact-confirmed `chordrift
  tracks exclude`; `chordrift tracks restore` is the reversible counterpart.

## Setup and complete reference

For installation, OAuth, archive recovery, embeddings, clustering, proposal
generation, artwork, retirement, bookmarks, enrichment, and every CLI leaf
command, see the [CLI command reference](reference/CLI_COMMANDS.md).

The roadmap and current implementation state are recorded in
[ROADMAP.md](../ROADMAP.md) and [CODEX_HANDOFF.md](../CODEX_HANDOFF.md).

Database-v2 migration operators can capture repeatable, provider-free baselines
with `chordrift db invariant-report`, `chordrift db storage-report`, and the
strictly non-mutating `chordrift db compact plan`. After installing the
additive schema, `chordrift db v2 status` compares current-state parity and
lists every still-blocked cutover gate. See the CLI reference and
[database-v2 design](design/DATABASE_ARCHITECTURE_V2.md) for the measured
restore rehearsal and retention boundaries.

The rehearsal migration itself has explicit provider-free phases:

```console
chordrift db v2 migration plan --account personal
chordrift db v2 migration apply --account personal --confirm <PLAN_SHA256>
chordrift db v2 migration verify --account personal
chordrift db v2 cutover-plan --account personal
```

Only `migration apply` moves database rows, and only after an exact hash match.
The other commands are read-only. A rehearsal cutover plan is evidence, not
production approval: do not reuse its hash after production state changes, do
not change the production connection, and do not delete legacy rows without a
new explicit plan/apply/verify approval.

Treat production as separate gates. First apply only additive schema/current-
state migrations 0040-0043, rerun the read-only reports, and stop to review the
production-emitted migration plan hash. Do not combine that schema gate with
`migration apply`, read cutover, observation-window start, or legacy cleanup.

After v0.1.3 runtime migration and a fresh invariant report, database-v1
storage cleanup has its own exact-confirmed provider-free phases:

```console
chordrift db compact cleanup plan --account personal
chordrift db compact cleanup apply --account personal --confirm <PLAN_SHA256>
chordrift db compact cleanup verify --account personal
```

`cleanup plan` is read-only. `cleanup apply` removes the superseded physical
provider bodies and legacy event/archive tables, so its hash is database-specific
and requires separate production approval. `cleanup verify` proves that the v2
invariant fingerprint is unchanged, normalized evidence counts match the
receipt, legacy table names are absent, and transient provider-import tables are
empty. Never copy a rehearsal cleanup hash to production without comparing the
production-emitted plan and obtaining explicit approval.
