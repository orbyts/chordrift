# Delete or exclude a track safely

Applies to the released v0.1.4 plan/readiness/apply workflow. V0.2 preserves
this safety boundary and will expose the same lifecycle through structured
application events.

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
currently surfacing. Then pull:

```console
$ chordrift sync pull --account personal
```

The pull records provider state. It does **not** immediately infer and commit an
exclusion. Build and inspect the next immutable plan:

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
| Remove from `Re-evaluate` | The corrective queue changed; this is not a global rejection by itself. |
| Remove from a protected user playlist | Provider-wins membership edit for that playlist. |
| Remove from one canonical and add to another | Likely move or destination preference; stage for review. |
| Delete an entire playlist | Ambiguous and potentially destructive; use explicit retirement policy. |

If you also want the track unsaved, remove it from Liked Songs in Spotify and
pull that state. Unsaving and excluding are independent decisions.

## When you mean “wrong playlist,” not “delete”

Do not remove the track as an exclusion. Add it to `Re-evaluate`, remove it from
the wrong destination, and follow
[Re-evaluate and reclassify a track](ROUTING_AND_RECLASSIFYING.md).
