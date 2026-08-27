# Delete or exclude a track safely

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

### Convenience scripts

When the plan contains only one `reconcile` phase, the complete routine wrapper
can perform the pull/plan/readiness/confirm/apply/pull/verify loop:

```console
$ scripts/chordrift-workflow.sh --account personal
```

When the exclusion is part of a combined plan, first inspect the plan manually,
then apply only its exact reconcile phase after any earlier publish phase has
been published and verified:

```console
$ scripts/chordrift-plan-phase.sh \
    --account personal --plan PLAN_ID --phase reconcile
```

The phase helper refuses reconcile while the same plan still contains publish,
and it refuses cleanup and retirement entirely. It retains the exact readiness
UUID confirmation instead of auto-approving the operation.

## Direct Neon exclusion for a known exception

Use direct exclusion only when there is no reliable provider gesture to
observe—for example, a provider-unavailable recording. This command records a
reversible exclusion in Neon and does not itself remove a Spotify item:

```console
$ chordrift tracks exclude --account personal \
    --spotify-id SPOTIFY_TRACK_ID \
    --reason "Provider-unavailable recording" \
    --confirm SPOTIFY_TRACK_ID
```

Restore returns the track to unresolved review; it does not guess a playlist:

```console
$ chordrift tracks restore --account personal \
    --spotify-id SPOTIFY_TRACK_ID \
    --reason "Available again; reconsider placement" \
    --confirm SPOTIFY_TRACK_ID
```

For an ordinary “I do not want this anymore” decision, prefer the verified
Spotify removal → pull → plan → reconcile path above because it preserves both
the provider gesture and Chordrift's interpretation.

## Spotify actions that do not mean exclusion

| Action | Meaning Chordrift can safely infer |
| --- | --- |
| Remove only from Liked Songs | The track is no longer saved. It may still belong in playlists. |
| Remove from a `Route — …` playlist | The corrective inbox changed; this is not a global rejection. |
| Remove from a protected user playlist | Provider-wins membership edit for that playlist. |
| Remove from one canonical and add to another | Likely move or destination preference; stage for review. |
| Delete an entire playlist | Ambiguous and potentially destructive; use explicit retirement policy. |

If you also want the track unsaved, remove it from Liked Songs in Spotify and
pull that state. Unsaving and excluding are independent decisions.

## When you mean “wrong playlist,” not “delete”

Do not remove the track as an exclusion. Add it to an appropriate
`Route — …` playlist and follow [Route and reclassify a track](ROUTING_AND_RECLASSIFYING.md).
