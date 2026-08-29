# Reclassify a track by moving it

Chordrift v0.2.0 does not use a special correction queue. When a track belongs
in a different managed playlist, make the natural edit in Spotify:

1. remove the track from the wrong Chordrift-managed playlist;
2. add it to the correct Chordrift-managed playlist; and
3. run the ordinary maintenance wizard.

```console
$ scripts/chordrift-maintain.sh --account personal
```

When exactly one new managed destination is visible, Chordrift interprets the
paired provider edits as one reclassification and shows it in the review as
`old destination -> new destination`. If the track appears in several new
managed destinations, the wizard asks which one is canonical. It then shows
one net-change summary and asks once before any provider write.

A direct move already completed in Spotify changes only Chordrift's canonical
intent; the wizard must not remove the track from its new destination. If
verification reveals different follow-up work, the wizard stops and requires a
new review instead of reusing the earlier confirmation.

The correction remains durable evidence in Neon. Today it changes canonical
placement without retraining a model. A future Classification Authority may
consume confirmed movement as reviewed training evidence through a separate,
versioned contract.

Removing a track from a managed playlist without adding it to another managed
destination means something different: the maintenance plan treats that as a
possible exclusion and asks you to review it. See
[Delete or exclude a track safely](DELETING_AND_EXCLUDING.md).

## Historical Re-evaluate retirement

`Re-evaluate` was the earlier correction queue. It is retired from ordinary
v0.2.0 use; its Neon rows and events remain as history. The hidden one-time
operator command refuses a non-empty queue and changes only Neon intent:

```console
$ chordrift reevaluate retire --account personal \
    --confirm "RETIRE RE-EVALUATE"
```

The resulting `retirement/archive_playlist` operation must still be inspected,
assessed, and explicitly applied before Spotify removes the empty playlist.
Daily maintenance deliberately refuses retirement operations.
