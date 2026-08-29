# How to use Chordrift

This is the user-facing entry point for Chordrift. Start with the task you want
to accomplish; use the comprehensive [CLI command reference](reference/CLI_COMMANDS.md)
only when you need every option or an internal diagnostic command.

These workflows describe **v0.2.0**, including the compatible maintenance CLI
and the provider-neutral product boundaries. Refer to the `v0.1.4` tag only for
the exact historical release. The personal deployment must cut over its binary
and verified 47/47 database together; see
[Recovery and rollback](how-to/RECOVERY_AND_ROLLBACK.md).

## Documentation map

| Need | Authoritative document | Status |
| --- | --- | --- |
| Perform daily library work | This guide and the linked `how-to/` pages | Current v0.2.0 maintenance behavior. |
| Understand IDs, phases, plan origins, and verification | [From intent to verified execution](how-to/INTENT_TO_EXECUTION.md) | Maintenance safety model plus V020-11R capability/origin reconciliation. |
| Look up a command | [CLI command reference](reference/CLI_COMMANDS.md) | Complete v0.2.0 command surface; operator-only leaves are labeled. |
| Recover or roll back an upgrade | [Recovery and rollback](how-to/RECOVERY_AND_ROLLBACK.md) | Atomic binary/database recovery and split-brain precautions. |
| Review the v0.2 product/client architecture | [Playlist product architecture](design/PLAYLIST_PRODUCT_ARCHITECTURE.md) | v0.2.0 portable core complete; hosted authority is next. |
| Review intent interpretation | [Platform interaction model](design/PLATFORM_INTENT_MODEL.md) | Active v0.2 product policy, grounded in the existing explicit CLI loop. |
| Review account/provider isolation | [Account and provider boundaries](design/ACCOUNT_AND_PROVIDER_BOUNDARIES.md) | Adversarial account/provider proof implemented; Spotify is the current production adapter. |
| Review the additive v0.2 schema | [Product schema foundation](design/PRODUCT_SCHEMA_V020_05.md) | Migration 0046 implemented and rehearsed only on isolated PostgreSQL 18; not on production Neon. |
| Review onboarding input capture | [Onboarding session boundary](design/ONBOARDING_SESSION_V020_06.md) | Provider-read-only boundary implemented; the released product CLI uses captured/fake inputs only. |
| Review the inventory-only onboarding result | [Inventory-only onboarding audit](design/ONBOARDING_AUDIT_V020_07.md) | Read-only Rust query implemented over captured current inventory; proposals remain unapproved and no CLI command was added. |
| Compare the extended-history onboarding result | [Enriched onboarding audit](design/ENRICHED_ONBOARDING_AUDIT_V020_08.md) | Same read-only inventory baseline plus one explicitly selected history import; strengthened claims carry exact support counts. |
| Review Discovery + Rediscovery selection | [Recipe execution](design/DISCOVERY_REDISCOVERY_RECIPE_V020_09.md) | Provider-neutral deterministic unordered draft consumed by the implemented V020-10 Spin orderer. |
| Review exact Spin ordering and replay | [Deterministic Spin preview](design/DETERMINISTIC_SPIN_PREVIEW_V020_10.md) | Exact ordered and persisted Rust value exposed by the provider-free product CLI. |
| Rehearse the v0.2 product through the CLI | [CLI-first product rehearsal](design/CLI_FIRST_PRODUCT_REHEARSAL_V020_11.md) | Opt-in development-line commands and installed-binary helper; fake/captured inputs and isolated migration-0046 database only. |
| Review recovered intake/apply compatibility | [Recovered intake compatibility](design/RECOVERED_INTAKE_COMPATIBILITY_V020_11R.md) | Enumerated writes, capability handshake, complete intake adapter, and maintenance/Spin origin separation. |
| Review approved Spin publication planning | [Spin publication-plan integration](design/SPIN_PUBLICATION_PLAN_V020_12.md) | Checkpoint-bound immutable plan plus fake-provider readiness/replay/verification; no production provider write path. |
| Review the newest live-state migration proof | [Latest-state migration rehearsal](design/LATEST_STATE_MIGRATION_REHEARSAL_V020_13.md) | Fresh read-only backup restored and migrated locally from 45/47 to 47/47 with exact invariant/domain parity; no candidate or cutover. |
| Review the verified candidate and cutover evidence | [Candidate and personal cutover gate](design/CANDIDATE_CUTOVER_GATE_V020_14.md) | Historical gate plus the completed v0.2.0 binary/database outcome; no Spotify write occurred. |
| Review the long-term classification/model vision | [Classification knowledge foundation](design/CLASSIFICATION_KNOWLEDGE_FOUNDATION.md) | Future shared canonical knowledge and versioned vectors plus an isolated private account overlay; lawful-input, consent, and evaluation prerequisites are explicit. |
| Review database-v2 decisions | [Database architecture v2](design/DATABASE_ARCHITECTURE_V2.md) | Completed v0.1.4 foundation and labeled historical execution record. |
| Review exact slice order | [Roadmap](../ROADMAP.md) | Authoritative execution map and completion checkboxes. |

If a historical release detail is needed, use the corresponding Git tag. The
documents on `main` prioritize unambiguous v0.2.0 operation and forward design.
The `v0.1.4` tag preserves its former documentation.

Chordrift treats Neon as the durable ledger and Spotify as the familiar
listening surface. A normal pull observes Spotify changes, but ambiguous intent
is staged for inspection rather than silently guessed.

## Everyday actions

| I want to… | Guide | Spotify action |
| --- | --- | --- |
| Add or discover a song | [Add songs and preserve discovery context](how-to/ADDING_AND_DISCOVERY.md) | Like/Save it; use a named intake only for a richer signal. |
| Review a mixed intake batch | [Add songs and preserve discovery context](how-to/ADDING_AND_DISCOVERY.md) | v0.2.0 capability-checked intake wizard. |
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

The v0.2.0 output groups the result into provider, current-library, and
listening-evidence tables and reports elapsed time for each phase. The provider
row also reports how many Spotify API requests the pull used. An unchanged
library is reused in Neon; only new Recently Played observations update
listening evidence and the affected per-track statistics. These timings are the
first diagnostic to share when a routine pull feels slow.

All interactive v0.2.0 commands use the same compact table, color, spacing, and
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
Current maintenance output must say `plan_origin: maintenance`; intake helpers
reject future Spin publication plans.

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

To record a reviewed discovery from `Inbox` in a proposed canonical destination
without writing to Spotify, use the separate installed-binary helper:

```console
$ scripts/chordrift-intake-move.sh --account personal \
    --to "Dakshina Pulse" --spotify-id SPOTIFY_TRACK_ID \
    --reason "Reviewed South Indian discovery"
```

It refuses active exclusions, non-Inbox tracks, and tracks that already have a
proposal disposition. It changes only editable Neon proposal intent and stops
before proposal approval, planning, source cleanup, or any provider write. See
[`scripts/README.md`](../scripts/README.md) for batching, stable-key usage, and
the guarded `--prepare` path when the previous proposal is already approved.

## Mixed-intake workflow

v0.2.0 exposes an exact machine-readable handshake:

```console
$ chordrift capabilities \
    --require maintenance.intake-workflow.v1 \
    --require maintenance.enumerated-playlist-additions.v1 \
    --require plan-origin.v1
```

The complete mixed-intake workflow is:

```console
$ scripts/chordrift-intake-wizard.sh --account personal
```

It pulls first, audits current Liked Songs and named intake against durable
intent/history, isolates verified removals, supports manual or reviewed
automatic placement, and advances only exact `maintenance` plans through
publish, verification, reconciliation, and separately confirmed cleanup. It
stops for unrelated unresolved tracks, new playlist/artwork design, retirement,
an incompatible binary, or a non-maintenance plan. `--review-only` performs the
joined audit without approval or provider writes.

Supporting helpers are `chordrift-manual-place.sh`,
`chordrift-cluster-unresolved.sh`, and `chordrift-plan-phase.sh`. They delegate
all domain decisions to the Rust CLI and require advertised capabilities before
doing work. Ordinary additions append only enumerated track IDs; complete
replacement remains exclusive to a verified membership-identical reorder.

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
surface. V020-10 now deterministically orders and persists that draft with exact
reasons, seed, fingerprints, capability snapshot, sections, warnings, and an
account-scoped query view. V020-11 now exposes these boundaries through the
opt-in, fixture-backed `product` CLI and installed-binary helper without adding
a provider action. V020-11R now adds the capability-checked intake adapter,
enumerated playlist writes, and explicit maintenance plan origins. V020-12 adds
approved, checkpoint-bound Spin plans and fake-provider verification without a
production mutation adapter. V020-13 proves the newest live state migrates
cleanly on an isolated restore. V020-14's fresh Neon candidate passed exact
parity and runtime gates, and V020-15 paired it with the released v0.2.0 binary
after preserving the former deployment for rollback. See the
[playlist product architecture](design/PLAYLIST_PRODUCT_ARCHITECTURE.md) for
the complete staged direction.
