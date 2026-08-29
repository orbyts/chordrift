# Recovered intake compatibility reconciliation — V020-11R

Status: implemented on the v0.2.0 development line. The released v0.1.4 tag
remains the reference for the installed daily driver.

## Recovered incident

The 92-track incident exposed a dangerous mismatch between an immutable plan
and its executor. A plan enumerating one ordinary addition entered a complete
desired-membership replacement path. That could remove unrelated live members
or restore a track the user had manually removed.

V020-11R selectively ports the correction from commits `9a078f3` and
`4b7d876`; neither maintenance branch was merged wholesale. Apply now has two
explicit strategies:

- ordinary `add_track` and `restore_track` operations append only their exact
  enumerated provider IDs, marking already-live IDs as reused; and
- `reorder_playlist` alone may replace complete order, only after proving live
  and desired membership are identical.

Pure regressions prove an ordinary addition cannot replace unrelated live
membership or restore an unenumerated manual removal.

## Operator workflow adapter

The recovered complete intake workflow is available through
`scripts/chordrift-intake-wizard.sh` and its manual-placement, reviewed-
clustering, and single-phase helpers. The shell owns prompting and sequencing
only. The Rust `intake audit` query owns the read-only join of current provider
intake with canonical coverage, proposal intent, exclusions, and listening
history. Existing Rust proposal, readiness, apply, and verification commands
remain authoritative for every state change.

The helper starts with observation, isolates verified removal intent, reviews
current intake, requires complete proposal coverage, and advances only fresh
maintenance plans through separately verified phases. It stops for unrelated
unresolved work, new playlist/artwork design, retirement, incompatible
binaries, and unsupported plan origins.

## Capability and plan-origin contract

`chordrift capabilities` emits one JSON `BinaryCapabilityManifest`. Repeated
`--require` values turn it into an exact exit-status handshake. Operator helpers
require stable feature names rather than guessing from the crate version.

Maintenance plans persist `plan_origin: maintenance` inside immutable
preconditions and print it in plan output. Maintenance readers and helpers
reject absent/unknown origins unless the plan is a recognized legacy instance
of the current maintenance planner. V020-12 Spin publication plans now persist
and expose `spin_publication`; they cannot enter the intake wizard.

This separation is architectural, not merely a shell check. Plan origin says
which business path authorized a diff; phase says which bounded risk class is
being executed. A Spin publication plan may eventually have a `publish` phase,
but that does not make it an intake-maintenance plan.

## Proof and safety boundary

Fake-installed-binary tests prove the complete review-only wizard sequence,
exact capability-first behavior for every recovered helper, and rejection of a
Spin publication origin before intake audit or apply. The intake audit retains
its isolated PostgreSQL read-only proof.

V020-11R performed no production Neon access, Spotify request/write, migration
0046 application, Spin publication implementation, crate publication, or
release. Compatibility preserves safety invariants and operator outcomes; it
does not force legacy command spelling or shell internals into the provider-
neutral Rust architecture.
