# Re-evaluate and reclassify a track

Applies to the released v0.2.0 workflow. The former multi-route workflow has
already been retired; `Re-evaluate` is the one current correction surface.

Use the single `Re-evaluate` playlist when you want to keep a track but reject
its current Chordrift destination. It is a temporary holding queue, not a final
playlist and not a preference signal.

## Capture while listening

In Spotify, move the playing track to `Re-evaluate`: add it there and remove it
from the wrong Chordrift destination. The visible library immediately matches
your judgment, while Chordrift retains the transition in Neon.

Then run:

```console
$ chordrift sync pull --account personal
$ chordrift reevaluate status --account personal
```

The pull records an immutable queue-entry event and current zero-signal queue
membership in Neon. While the track remains there, Chordrift neither infers an
exclusion nor restores the rejected source membership.

## Review a longer queue in CSV

```console
$ chordrift reevaluate export \
    --account personal \
    --file reevaluate.csv
```

Add only classifications you actually know, mark changed rows with
`action=set`, and import/approve through the existing classification batch
workflow.

## Resolve the queue interactively

Use the dedicated capability-checked wizard when the destination is an existing
canonical playlist:

```console
$ scripts/chordrift-reevaluate-wizard.sh --account personal
```

It performs a fresh pull, lists the authoritative provider-observed queue,
prepares an editable copy of the approved proposal, and asks you to choose an
exact existing destination or defer each track. It records explicit assignment
revisions first. Only after complete proposal and unchanged-artwork review does
it offer to publish the selected additions, verify them, and separately remove
those tracks from an old incorrect destination only when the plan identifies a
snapshot-matched `managed_provider_drift` removal. It verifies that correction
before separately removing the verified tracks from `Re-evaluate` with an exact
destructive-cleanup confirmation.

The wizard rejects unrelated unresolved tracks, new playlist or artwork design,
retirement, non-maintenance plans, and unexpected publish, reconcile, or cleanup
operations.
Review without changing proposal intent or Spotify with:

```console
$ scripts/chordrift-reevaluate-wizard.sh \
    --account personal --review-only
```

If the proposal was already approved and the wizard stopped later—for example,
at an artwork confirmation—resume without repeating the placement decisions:

```console
$ scripts/chordrift-reevaluate-wizard.sh --account personal --resume
```

Exact-ID prompts retry after a typo and accept `cancel` as the explicit safe
exit. Reusing existing artwork validates the original files in place through a
temporary manifest; it does not copy the complete artwork directory.

The general `chordrift-intake-wizard.sh` is not the Re-evaluate placement
wizard. It handles Liked Songs, named intake, and verified removals from managed
playlists.

## Create the queue

The queue has one stable identity, a label-free artwork master, and a
provider-labeled artwork artifact.

```console
$ chordrift artwork render \
    --background artwork/review/re-evaluate-background.png \
    --title "Re-evaluate" \
    --output artwork/review/re-evaluate-spotify.png

$ chordrift reevaluate create \
    --account personal \
    --background artwork/review/re-evaluate-background.png \
    --artwork artwork/review/re-evaluate-spotify.png
```

## Reconciliation invariant

A non-empty Re-evaluate queue means pending work. Later review must either:

1. move the track into an existing canonical playlist and record the correction;
2. promote a coherent cohort into a new poetic canonical playlist with its own
   identity and artwork; or
3. leave it in Re-evaluate because the destination remains uncertain.

Removing a track from both its verified source and Re-evaluate is the explicit
provider gesture from which Chordrift may stage a reversible exclusion. Queue
history remains in Neon even after the current queue membership disappears.

Chordrift clears queue membership only after the selected destination has been
published and verified. The cleanup is additionally gated on a manual
assignment revision newer than the queue-entry event and a destination that is
different from the rejected source. Merely regenerating a proposal cannot
silently clear the queue.

The general manual-placement helper remains available when the guided workflow
must be resumed one step at a time:

```console
$ scripts/chordrift-manual-place.sh --account personal \
    --to "NEW DESTINATION" --spotify-id SPOTIFY_TRACK_ID \
    --reason "Reviewed Re-evaluate correction"
```

It requires the installed-binary capability handshake, refuses active
exclusions, and changes editable proposal intent only. Maintenance phase
helpers also require `plan_origin: maintenance`; future Spin publication plans
cannot enter this correction workflow.

## Add a private classification dimension

The complete column glossary, recommended vocabulary, examples, and v0.2
native-client token direction live in
[Classify tracks with user dimensions](CLASSIFICATION_DIMENSIONS.md).

Use explicit classification when provider/public metadata is missing or your
own cultural grouping should outrank sound similarity. These facts are private,
revisioned, and separate from public or inferred facts. They affect the
personalized embedding without rewriting the base acoustic embedding.

For one known track:

```console
$ chordrift classify set \
    --spotify-id SPOTIFY_TRACK_ID \
    --collection south-asian \
    --region south-indian \
    --tradition film \
    --cohort ar-rahman-favorites \
    --language ta \
    --reason "Tamil film song; keep out of general ambient grouping"
```

For a handful with the same verified dimensions, repeat `--spotify-id` in one
command. Chordrift resolves the whole set first and commits it atomically:

```console
$ chordrift classify set \
    --spotify-id FIRST_ID --spotify-id SECOND_ID --spotify-id THIRD_ID \
    --collection south-asian --region north-indian --tradition film \
    --language hi --reason "Reviewed together as Hindi film songs"
```

Use `region` for cultural/geographic grouping and `tradition` for musical form.
For example, `south-indian` is a region while `carnatic-classical` is a
tradition. Repeat a dimension when multiple values apply. Values normalize to
lowercase hyphenated slugs. `--notes` stores context but is not embedded.

Review or reverse the decision without losing provenance:

```console
$ chordrift classify history --spotify-id SPOTIFY_TRACK_ID
$ chordrift classify clear --spotify-id SPOTIFY_TRACK_ID \
    --reason "classification replaced after listening review"
```

For a larger review, export one or more current playlists into one deduplicated
CSV:

```console
$ chordrift classify export \
    --playlist "Dakshina Pulse" \
    --playlist "Uttara Glow" \
    --file data/review/south-asian-classification.csv
```

The worksheet includes track identity, album, limited inferred release facts,
and current `user_*` values. It is inert: edit only `user_*`, place `set` or
`clear` in `action`, and add a reason to every changed row. Blank action means
no change. Import creates a draft and changes no active facts:

```console
$ chordrift classify import --file data/review/south-asian-classification.csv
$ chordrift classify approve --batch BATCH_ID --confirm BATCH_ID
$ chordrift embeddings generate
```

Only exact-ID approval activates the batch. Classification does not immediately
move Spotify tracks; use it to create/assign the next proposal, then follow the
normal reviewed plan and apply workflow.
