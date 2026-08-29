# Chordrift helper scripts

This directory contains operator and development conveniences around the
installed Chordrift application. These scripts are not part of the Rust CLI's
public product surface, and the future UI does not depend on them. The workflow
wrapper remains a supported operator convenience for the v0.2.0 maintenance
surface. It retains the proven v0.1.4 safety outcomes while using the current
application facade and explicit plan-origin checks.

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

## Provider-free v0.2 product rehearsal

`chordrift-product-rehearsal.sh` uses an installed v0.2.0 binary to
exercise the V020-06 through V020-10 application boundaries as one workflow:

```console
$ CHORDRIFT_PRODUCT_REHEARSAL=1 \
  CHORDRIFT_BIN=/path/to/development/chordrift \
  scripts/chordrift-product-rehearsal.sh \
    --account CHORDRIFT_ACCOUNT_UUID \
    --recipe-revision RECIPE_REVISION_UUID \
    --onboarding-fixture onboarding.json \
    --spin-fixture spin.json
```

Use only an isolated database where migration 0046 was already applied. The
helper never runs a migration. It captures and compares inventory-only and
explicitly enriched onboarding audits, reviews collections and the immutable
recipe, executes selection, creates the exact ordered Spin, and proves that a
reload retains its fingerprint. It invokes only `chordrift product` commands;
there is no provider command, publication approval, or provider write.

This is a v0.2 development proof, not a v0.1.4 daily-driver workflow. See
[`CLI_FIRST_PRODUCT_REHEARSAL_V020_11.md`](../docs/design/CLI_FIRST_PRODUCT_REHEARSAL_V020_11.md)
for fixture and safety boundaries.

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

## V020-11R mixed-intake workflow

The following helpers require a compatible v0.2.0 binary. They call
`chordrift capabilities --require …` before operational commands and fail
closed when the installed binary lacks the exact workflow, enumerated-addition,
or plan-origin contract. A version string alone is never treated as proof.

Run the complete guided workflow with:

```console
$ CHORDRIFT_BIN=/path/to/chordrift \
    scripts/chordrift-intake-wizard.sh --account personal
```

The wizard performs a fresh pull by default, requires every plan to report
`plan_origin: maintenance`, audits current Liked Songs and named intake through
the Rust `intake audit` query, isolates reversible exclusion intent, and walks
through manual or reviewed automatic placement. After complete proposal and
artwork review, it advances fresh plans through publish, pull/verification,
reconciliation, and separately confirmed cleanup. It refuses unrelated
unresolved tracks, new playlist/artwork design, retirement, repeated no-progress
phases, and future Spin publication plans. `--review-only` stops after the
read-only audit and may run without an interactive terminal.

Supporting adapters remain independently useful:

```console
$ scripts/chordrift-manual-place.sh --account personal \
    --to "Dakshina Pulse" --spotify-id SPOTIFY_TRACK_ID \
    --reason "Reviewed South Indian discovery"
$ scripts/chordrift-cluster-unresolved.sh --account personal
$ scripts/chordrift-plan-phase.sh --account personal \
    --plan PLAN_ID --phase publish
```

Manual placement changes editable Neon intent only. Clustering is read-only by
default, reserves intake/Re-evaluate tracks unless `--include-intake` is
explicit, and requires the exact proposal UUID for apply mode. The phase helper
accepts only maintenance `publish` or `reconcile`, retains readiness and exact
assessment confirmation, verifies delayed provider observation with bounded
pulls, and refuses cleanup/retirement.

The enumerated-write correction is below the scripts: ordinary `add_track` and
`restore_track` operations append only their named IDs. They never replace the
complete desired membership, so an unrelated live track cannot disappear and a
manually removed track cannot return without its own explicit restoration.
Only `reorder_playlist` may use complete replacement, after proving live and
desired membership are identical.

These are compatibility adapters around Rust-owned commands, not a second
business path. Future clients may present the workflow differently while
preserving the same safety outcomes.

## Re-evaluate correction workflow

Resolve provider-observed `Re-evaluate` tracks into existing canonical
destinations with:

```console
$ scripts/chordrift-reevaluate-wizard.sh --account personal
```

Use `--review-only` for the fresh-pull and queue report without proposal intent,
approval, or provider writes; use `--skip-pull` only when you have independently
confirmed the current snapshot. The wizard prepares an editable proposal,
records each explicit replacement through `chordrift-manual-place.sh`, refuses
unrelated unresolved work, reuses only the unchanged approved artwork set, and
allows only enumerated selected-track publication. After provider verification,
it presents a separate exact-confirmed destructive cleanup containing only
selected removals from `Re-evaluate`.

Do not use `chordrift-intake-wizard.sh` to choose Re-evaluate destinations. The
intake wizard handles Liked Songs, named intake playlists, and verified managed-
playlist removals. `chordrift-workflow.sh` and `chordrift-plan-phase.sh` are
lower-level resume/recovery helpers, not the first command for queue review.

## Artwork label renderer

`render_artwork_label.swift` is an internal macOS development helper for
rendering deterministic artwork labels. It is used while preparing repository
artwork assets and is unrelated to routine synchronization.
