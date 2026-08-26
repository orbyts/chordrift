# Chordrift helper scripts

This directory contains operator and development conveniences around the
installed Chordrift application. These scripts are not part of the Rust CLI's
public product surface, and the future UI does not depend on them.

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

## Artwork label renderer

`render_artwork_label.swift` is an internal macOS development helper for
rendering deterministic artwork labels. It is used while preparing repository
artwork assets and is unrelated to routine synchronization.
