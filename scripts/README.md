# Chordrift helper scripts

This directory contains operator and development conveniences around the
installed Chordrift application. These scripts are not part of the Rust CLI's
public product surface, and the future UI does not depend on them. The workflow
wrapper remains a supported operator convenience for the installed v0.1.4
daily driver while `main` advances through the v0.2 application-facade work.

## Safe synchronization workflow

`chordrift-workflow.sh` runs the routine terminal workflow with the installed
`chordrift` binary:

```console
$ scripts/chordrift-workflow.sh --account personal
```

It never invokes `cargo run`. Set `CHORDRIFT_BIN` only when you need to select
another installed executable explicitly:

```console
$ CHORDRIFT_BIN=/path/to/chordrift scripts/chordrift-workflow.sh --account personal
```

The script performs these steps:

1. Pull current Spotify state into Neon.
2. Create and display an immutable plan.
3. Run publish preflight when the plan's phase is `publish`.
4. Run readiness with read-only Spotify probes.
5. Require the operator to type the exact readiness-assessment UUID.
6. Apply one `publish` or `reconcile` phase.
7. Pull again and display the verified apply receipt.
8. Create and display a final convergence plan.

Use `--skip-initial-pull` only immediately after you have already completed and
inspected a pull:

```console
$ scripts/chordrift-workflow.sh --account personal --skip-initial-pull
```

Spotify can briefly return the previous playlist snapshot immediately after a
manual edit. When an interactive run finds a zero-operation plan, the wrapper
now explains that possibility and lets you type `retry` after waiting a moment.
For bounded unattended polling, supply the maximum wait in seconds; the wrapper
retries every 10 seconds and never waits longer than 10 minutes:

```console
$ scripts/chordrift-workflow.sh --account personal --wait-for-change 90
```

This polling mode is intended for a workflow started immediately after a known
Spotify edit. Omit it for an ordinary observation where zero operations is an
expected successful result.

The wrapper stops without applying when a plan is stale, readiness fails, the
confirmation does not match, the plan spans multiple phases, or the phase is
`cleanup` or `retirement`. Those destructive phases retain their separate
manual review and approval workflows. The wrapper does not auto-confirm an
assessment and does not bypass any core Chordrift safety gate.

Redirected Chordrift output remains the stable plain key/value and tabular
format used internally by the wrapper. Interactive plan, readiness, apply, and
pull reports still use Chordrift's normal formatted terminal presentation.

## Reviewed Inbox placement

`chordrift-intake-move.sh` records one or more reviewed Inbox discoveries in an
existing editable proposal using the installed `chordrift` binary. Resolve the
destination by its exact display name:

```console
$ scripts/chordrift-intake-move.sh \
    --account personal \
    --to "Dakshina Pulse" \
    --spotify-id SPOTIFY_TRACK_ID \
    --reason "Reviewed A. R. Rahman discovery"
```

Repeat `--spotify-id` for a reviewed batch. `--playlist STABLE_KEY` is also
available when you prefer the durable proposal identity over its display name.

If the latest proposal is already approved, it is immutable. The helper stops
and asks you to repeat the reviewed command with `--prepare`. That option is
accepted only when the supplied IDs are the complete unresolved set. It then
uses Chordrift's strict `proposals extend --min-similarity 1` path to preserve
the approved playlist structure in a new editable proposal before recording
the explicit destinations. This guard prevents an unrelated unresolved track
from receiving a new centroid placement as a side effect. Chordrift also
replays all durable manual decisions into the editable copy; that can expose
older needs-review decisions that were masked by the previously approved
generation. The helper lists any such unresolved tracks after recording the
requested Inbox placement and does not classify them automatically. Any
exact-match automatic placement among the supplied set is replaced by the
explicit destination you selected.

Before recording an assignment, the helper resolves one unambiguous
destination and verifies that every track is currently in Inbox, is unresolved,
and has no active exclusion. It records only manual assignment intent. It does
not approve the proposal, create or apply a synchronization plan, remove the
source item from Inbox, or write to Spotify. Afterward it shows the proposal
UUID and coverage state so the entire proposal can receive its normal separate
review and approval.

This distinction matters: approving a proposal approves all of its current
contents, not only the tracks supplied to this helper. Once the complete
proposal is reviewed and approved, use `chordrift-workflow.sh` for the ordinary
plan/readiness/confirm/apply/verify sequence.

## General manual placement

`chordrift-manual-place.sh` records an explicit destination for one or more
tracks in the latest editable proposal. Unlike the stricter Inbox helper, it
also supports correcting a generated placement after you inspect it:

```console
$ scripts/chordrift-manual-place.sh \
    --account personal \
    --to "Dakshina Pulse" \
    --spotify-id SPOTIFY_TRACK_ID \
    --reason "Reviewed Telugu discovery"
```

The helper resolves the display name to one durable playlist key, rejects
ambiguous names and active exclusions, then delegates the assignment to the
Rust CLI. It changes proposal intent in Neon only. It does not approve the
proposal, modify a Spotify playlist, or remove an item from an intake playlist.

Use `chordrift-intake-move.sh` when you specifically want its stricter proof
that every supplied track is currently unresolved in Inbox. Use this general
helper for Liked/Re-evaluate review or for a deliberate correction.

## Cluster the remaining unresolved tracks

After manually assigning culturally specific or otherwise personal decisions,
`chordrift-cluster-unresolved.sh` applies Chordrift's existing clustering
criteria to the remainder. Its default invocation is a read-only audit:

```console
$ scripts/chordrift-cluster-unresolved.sh --account personal
```

It prints the exact proposal UUID needed for the mutating form:

```console
$ scripts/chordrift-cluster-unresolved.sh \
    --account personal \
    --apply \
    --confirm PROPOSAL_UUID
```

By default the script stops if unresolved evidence still comes from Inbox,
Liked/Saved, intake, or Re-evaluate. That reserves those tracks for your manual
review. `--include-intake` is an explicit override for a batch you have already
reviewed. Apply mode records generated destinations durably in the proposal;
it does not approve the proposal or write to Spotify.

## Apply one phase of a combined plan

The routine workflow wrapper intentionally stops when a plan spans several
phases. `chordrift-plan-phase.sh` handles one reviewed `publish` or `reconcile`
phase while preserving the same safety sequence:

```console
$ scripts/chordrift-plan-phase.sh \
    --account personal \
    --plan PLAN_UUID \
    --phase publish
```

The helper displays the exact plan, runs publish preflight when needed, probes
readiness, and requires you to type the exact assessment UUID. After the one
Spotify-writing phase it pulls current provider state, verifies the apply
receipt, and creates a new plan from that new snapshot. Run it again with the
new plan UUID if a later `reconcile` phase remains.

It refuses stale plans, `cleanup`, `retirement`, and reconcile while publish
operations remain in the same plan. Those restrictions are intentional; the
script is a convenience, not a bypass around Chordrift's safety model.

## Guided mixed-intake workflow

`chordrift-intake-wizard.sh` is the common interactive path for a batch spread
across Liked Songs, Inbox, From Friends, Liked from Radio, and From Prompts:

```console
$ scripts/chordrift-intake-wizard.sh --account personal
```

It starts with a fresh pull. Before intake placement it creates an immutable
plan and distinguishes `exclude_track` intent caused by verified user removals
from routine provider-drift duplicate removals. With one exact plan
confirmation it records the former as reversible Neon exclusions without a
Spotify write. It holds the latter, plus existing publication work, until
intake coverage is complete; this avoids whole-library readiness deadlocks.

The second stage calls `chordrift intake audit`, a read-only Rust query over the
exact current provider snapshot plus Neon intent, exclusions, and normalized
history. It walks through active exclusions, private/manual placements, normal
existing-playlist suggestions, complete-proposal approval, unchanged artwork
reuse, phased publication, routine reconciliation, verification, and
exact-confirmed destructive intake cleanup. Unresolved items are matched to the
fresh intake audit by exact Spotify identity, never inferred from display text.
`--review-only` stops after the joined classification report.

The wizard intentionally stops when it encounters unrelated unresolved tracks,
a missing existing-playlist suggestion, a new playlist, retirement, or artwork
that cannot be reused unchanged. Those are separate creative or destructive
decisions, not intake automation. Like every helper here, it uses the installed
binary selected by `CHORDRIFT_BIN`; it contains no SQL, Spotify API client, or
clustering implementation.

## Artwork label renderer

`render_artwork_label.swift` is an internal macOS development helper for
rendering deterministic artwork labels. It is used while preparing repository
artwork assets and is unrelated to routine synchronization.
