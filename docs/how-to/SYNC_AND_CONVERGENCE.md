# Synchronize and prove convergence

This is the released v0.1.4 safety workflow and remains the daily operational
path during v0.2 development. The v0.2 application facade must preserve it
exactly before any new product workflow is added.

Use this workflow after provider edits or whenever Chordrift needs to publish an
approved Neon change.

## Observe Spotify

```console
$ chordrift sync pull --account personal
```

Pull imports current provider state, reuses unchanged playlist bodies and saved
tracks from Neon, collects incremental Recently Played observations, refreshes
analysis/history links, and verifies awaiting apply runs. Pull itself does not
silently approve ambiguous intent.

When the saved-track count or leading membership changes, Spotify must be read
far enough to prove the current complete membership. Chordrift then resolves
already-known provider records from Neon as a set, writes only new or changed
metadata, and batches the immutable snapshot membership. Interactive terminals
show progress bars for these phases. A redirected command keeps plain progress
and tab-separated tables so logs and scripts do not receive ANSI control codes.

## Plan without provider writes

```console
$ chordrift sync plan --account personal
$ chordrift sync plan-show --account personal --details
```

Read every operation. Confirm the phase, playlist, track, and whether anything
is destructive. A zero-operation plan proves current convergence.

## Validate the exact plan

```console
$ chordrift sync apply-preflight --account personal --plan PLAN_ID
$ chordrift sync readiness --account personal --plan PLAN_ID --probe
```

Preflight validates local artwork and request estimates without contacting
Spotify. Readiness checks the current snapshot, complete library coverage,
approval gates, restart behavior, idempotence, retry policy, and—with `--probe`—
the read-only provider identity and OAuth scopes.

## Apply one bounded phase

```console
$ chordrift sync apply --account personal \
    --assessment ASSESSMENT_ID \
    --phase publish \
    --confirm ASSESSMENT_ID
```

Use `publish` for approved playlist creation, membership, ordering, and artwork;
use `reconcile` for a reviewed managed-state interpretation such as exclusion.
Cleanup and retirement are destructive phases and require their additional
explicit gates. Never substitute a plan ID for `--confirm`: it must repeat the
assessment ID.

## Prove the provider accepted it

```console
$ chordrift sync pull --account personal
$ chordrift sync apply-show --account personal
$ chordrift sync plan --account personal
```

The apply run should be `succeeded`. The next plan should have zero operations
for the surfaces just published. If an apply remains `awaiting_pull`, do not
repeat it blindly; inspect provider and Neon state first.
