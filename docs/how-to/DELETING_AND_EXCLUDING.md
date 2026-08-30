# Delete or exclude a track safely

Applies to the v0.2.1-alpha.13 provider-first maintenance workflow. The
application facade preserves this safety boundary and exposes the lifecycle
through structured events.

Use this workflow when a resurfaced track is something you no longer want in
active Chordrift listening playlists. In Chordrift terminology this is an
**exclusion**, not historical erasure.

## What exclusion preserves

Exclusion keeps the track's identity, listening history, provenance, previous
playlist assignment, and the reason it was excluded in Neon. This prevents a
later clustering run from quietly rediscovering and republishing the track. The
decision remains explainable and reversible.

## Remove it from a verified Chordrift playlist

In Spotify, remove the track from the Chordrift-managed playlist where it is
currently surfacing. Then run the ordinary wrapper:

```console
$ scripts/chordrift-maintain.sh --account personal
```

The wrapper records the complete provider state, compares it with the last
exactly accepted baseline, and records the missing managed membership as an
active exclusion. It never restores the track from an older desired-state
proposal. If the change is ambiguous, it stops for a bounded decision instead
of guessing.

The following low-level plan workflow remains a developer/recovery path. Build
and inspect the immutable plan:

```console
$ chordrift sync plan --account personal
$ chordrift sync plan-show --account personal --details
```

For the intended track, verify that the reconcile phase contains an
`exclude_track` operation. If the plan instead proposes something surprising,
stop: the provider edit was ambiguous or the playlist lacked a verified managed
baseline.

Run readiness against the exact plan:

```console
$ chordrift sync readiness --account personal --plan PLAN_ID --probe
```

When every check passes, apply only the reconcile phase. Repeat the assessment
ID as the explicit confirmation:

```console
$ chordrift sync apply --account personal \
    --assessment ASSESSMENT_ID \
    --phase reconcile \
    --confirm ASSESSMENT_ID
```

Pull once more and inspect the track:

```console
$ chordrift sync pull --account personal
$ chordrift tracks inspect --spotify-id SPOTIFY_TRACK_ID
```

The report should show an active exclusion and retain its earlier history.

List all active exclusions at any time without contacting Spotify:

```console
$ chordrift tracks exclusions --account personal
```

## Empty the exclusion archive

The archive behaves like a reversible trash/archive disposition, not destructive
database erasure. Once every excluded track is absent from the newest complete
provider observation, clear all active dispositions with exact account
confirmation:

```console
$ chordrift tracks empty-exclusions --account personal --confirm personal
```

This command performs no provider write. It retains the track, historical
exclusion row, reason, and timestamps, resolves the visible archive item, and
creates an internal forget tombstone while superseding stale placement intent.
The old model therefore cannot replay it. The command is all-or-nothing and
refuses if an excluded track still appears in a current playlist or saved
tracks.

On the V020-11R development line, ordinary publish additions can append only
the track IDs enumerated by the reviewed plan. They cannot replace complete
playlist membership or restore a manually removed track merely because that
track remains in an older desired-state document. Full replacement is reserved
for an explicit reorder whose live and desired memberships are identical.

For one reviewed publish or reconcile phase in a combined maintenance plan:

```console
$ scripts/chordrift-plan-phase.sh --account personal \
    --plan PLAN_ID --phase reconcile
```

The helper requires the binary capability handshake and
`plan_origin: maintenance`; it refuses cleanup, retirement, stale plans, and
future Spin publication plans.

## Spotify actions that do not mean exclusion

| Action | Meaning Chordrift can safely infer |
| --- | --- |
| Remove only from Liked Songs | The track is no longer saved. It may still belong in playlists. |
| Remove from a protected user playlist | Provider-wins membership edit for that playlist. |
| Remove from one canonical and add to exactly one other | Direct reclassification; preserve the correction as evidence. |
| Delete an entire playlist | Ambiguous and potentially destructive; use explicit retirement policy. |

If you also want the track unsaved, remove it from Liked Songs in Spotify and
pull that state. Unsaving and excluding are independent decisions.

## When you mean “wrong playlist,” not “delete”

Do not leave the track only absent. In Spotify, move it from the wrong managed
playlist directly to the correct managed playlist, then follow
[Reclassify a track by moving it](ROUTING_AND_RECLASSIFYING.md).
