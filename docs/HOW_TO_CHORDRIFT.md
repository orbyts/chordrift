# How to use Chordrift

This is the user-facing entry point for Chordrift. Start with the task you want
to accomplish; use the comprehensive [CLI command reference](reference/CLI_COMMANDS.md)
only when you need every option or an internal diagnostic command.

These workflows describe the released **v0.1.4** daily driver and remain valid
while `main` develops v0.2.0. The v0.2 application facade now carries the same
CLI behavior, but the released binary and every command/output remain unchanged.
Design pages identify which v0.2 foundations are implemented or still planned.

## Documentation map

| Need | Authoritative document | Status |
| --- | --- | --- |
| Perform daily library work | This guide and the linked `how-to/` pages | Released v0.1.4 behavior; preserved during current v0.2 work. |
| Look up a command | [CLI command reference](reference/CLI_COMMANDS.md) | Complete v0.1.4 command surface; historical operator-only leaves are labeled. |
| Review the v0.2 product/client architecture | [Playlist product architecture](design/PLAYLIST_PRODUCT_ARCHITECTURE.md) | V020-01 through V020-09 implemented; V020-10 next; later behavior explicitly listed. |
| Review intent interpretation | [Platform interaction model](design/PLATFORM_INTENT_MODEL.md) | Active v0.2 product policy, grounded in the existing explicit CLI loop. |
| Review account/provider isolation | [Account and provider boundaries](design/ACCOUNT_AND_PROVIDER_BOUNDARIES.md) | Test-only adversarial proof implemented; production adapters remain v0.1.4. |
| Review the additive v0.2 schema | [Product schema foundation](design/PRODUCT_SCHEMA_V020_05.md) | Migration 0046 implemented and rehearsed only on isolated PostgreSQL 18; not on production Neon. |
| Review onboarding input capture | [Onboarding session boundary](design/ONBOARDING_SESSION_V020_06.md) | Provider-read-only application boundary implemented with a fake provider and isolated PostgreSQL 18; no released CLI command. |
| Review the inventory-only onboarding result | [Inventory-only onboarding audit](design/ONBOARDING_AUDIT_V020_07.md) | Read-only Rust query implemented over captured current inventory; proposals remain unapproved and no CLI command was added. |
| Compare the extended-history onboarding result | [Enriched onboarding audit](design/ENRICHED_ONBOARDING_AUDIT_V020_08.md) | Same read-only inventory baseline plus one explicitly selected history import; strengthened claims carry exact support counts. |
| Review Discovery + Rediscovery selection | [Recipe execution](design/DISCOVERY_REDISCOVERY_RECIPE_V020_09.md) | Provider-neutral deterministic unordered draft; exact ordered Spin and CLI remain later slices. |
| Review database-v2 decisions | [Database architecture v2](design/DATABASE_ARCHITECTURE_V2.md) | Completed v0.1.4 foundation and labeled historical execution record. |
| Review exact slice order | [Roadmap](../ROADMAP.md) | Authoritative execution map and completion checkboxes. |

If a historical release detail is needed, use the corresponding Git tag. The
documents on `main` prioritize unambiguous current v0.1.4 operation and ongoing
v0.2 review.

Chordrift treats Neon as the durable ledger and Spotify as the familiar
listening surface. A normal pull observes Spotify changes, but ambiguous intent
is staged for inspection rather than silently guessed.

## Everyday actions

| I want to… | Guide | Spotify action |
| --- | --- | --- |
| Add or discover a song | [Add songs and preserve discovery context](how-to/ADDING_AND_DISCOVERY.md) | Like/Save it; use a named intake only for a richer signal. |
| Inventory or retire saved albums | [Saved albums and album cleanup](how-to/SAVED_ALBUMS.md) | Archive-only retirement keeps immutable album and track history. |
| Stop hearing a song | [Delete or exclude a track safely](how-to/DELETING_AND_EXCLUDING.md) | Remove it from its verified Chordrift playlist, then reconcile. |
| Keep a song but reject its current vibe | [Re-evaluate and reclassify a track](how-to/ROUTING_AND_RECLASSIFYING.md) | Move it to `Re-evaluate` and remove the wrong destination. |
| Add private region, tradition, language, or cohort facts | [Classify tracks with user dimensions](how-to/CLASSIFICATION_DIMENSIONS.md) | Review one track/a small group directly, or approve a CSV batch. |
| Bring Neon up to date | [Synchronize and prove convergence](how-to/SYNC_AND_CONVERGENCE.md) | Run a pull after provider changes. |
| Understand how the product interprets provider intent | [Platform interaction model](design/PLATFORM_INTENT_MODEL.md) | Keep using Spotify; Chordrift interprets bounded changes. |
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
If you start it immediately after a known Spotify edit, use
`--wait-for-change 90` to retry a briefly stale provider snapshot every ten
seconds for at most 90 seconds. Interactive zero-operation runs also offer a
manual retry before accepting convergence.
The complete operator/developer reference is in
[`scripts/README.md`](../scripts/README.md).

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

V0.2 recipe playlists are a separate layer from both intake and canonical
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

The v0.2 roadmap and current implementation state are recorded in
[ROADMAP.md](../ROADMAP.md) and [CODEX_HANDOFF.md](../CODEX_HANDOFF.md).

Database v2 migration, cutover, cleanup, and old-project retirement are already
complete for the released runtime. Their exact-confirmed commands remain in the
binary for audit and recovery, but they are not part of everyday use and must
not be replayed as a new production procedure. Read-only diagnostics such as
`chordrift db status`, `db invariant-report`, `db storage-report`, `db v2
status`, and `db compact cleanup verify` remain useful. The chronological
[database-v2 design](design/DATABASE_ARCHITECTURE_V2.md) preserves the measured
rehearsal and production record and clearly marks its intermediate gates as
historical.

## v0.2 development direction

V020-01 is implemented in the public Rust `contract` module. It supplies the
provider- and transport-neutral shapes that the CLI, hosted service, and future
native clients will share. V020-02 adds one public application facade and
routes current CLI handlers through it without changing commands, redirected
output, interactive presentation, database behavior, or provider behavior.
V020-03 adds the provider-neutral ownership, identity, capability, collection,
surface, recipe-v1, and Spin value vocabulary without changing the CLI. V020-04
adds the deterministic test-only isolation and fake-provider proof without
changing runtime behavior. V020-05 adds and locally rehearses the additive
product schema without applying it to production Neon. V020-06 captures a
selected immutable provider inventory and optional extended evidence through a
read-only provider port, persists exact input/provenance manifests, ignores
current Chordrift intent, and does not add a CLI surface. V020-07 reads only
that immutable checkpoint and returns deterministic library, overlap,
capability, uncertainty, and preserve-first proposal values. It neither writes
intent nor infers listening behavior, and it also adds no CLI surface. The
V020-08 adds the same inventory baseline plus one explicitly fingerprinted
extended-history import, and reports exactly which listening conclusions gained
direct record support. It still writes no intent and adds no CLI surface.
V020-09 now turns immutable recipe inputs into a deterministic unordered
selection draft, with weighted source allocation, eligibility, hard boundaries,
track/artist budgets, familiarity capacity, section capacity, and visible
capability degradation. It performs no provider/database write and adds no CLI
surface. The exact ordered and persisted Spin preview is next in V020-10. See the
[playlist product architecture](design/PLAYLIST_PRODUCT_ARCHITECTURE.md) for
the complete staged direction.
