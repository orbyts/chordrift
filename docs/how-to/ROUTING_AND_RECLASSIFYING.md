# Route and reclassify a track

Use a route when you want to keep a track but reject its current Chordrift
destination. A route is a temporary corrective inbox, not a final playlist and
not a preference signal.

## Capture while listening

Add the playing track in Spotify to the most truthful route, such as:

- `Route — South Indian`;
- `Route — North Indian`;
- `Route — Decide Later` when the current placement feels wrong but you do not
  yet know the destination.

Do not also remove it from its current Chordrift playlist. Keeping the source
membership until verified reassignment makes interruption harmless.

Then run:

```console
$ chordrift sync pull --account personal
$ chordrift routes tracks --account personal --route "South Indian"
```

The pull records the provider addition as durable, zero-signal route membership
in Neon.

## Add from the CLI instead

```console
$ chordrift routes add \
    --account personal \
    --route "Decide Later" \
    --spotify-id SPOTIFY_TRACK_ID \
    --reason "Current vibe feels wrong; destination undecided"
```

This changes Neon only. Use the normal plan/readiness/publish workflow to make
the route membership visible in Spotify.

## Create a new route

Every route needs its own meaning, description, label-free artwork master, and
provider-labeled artwork. Artwork should follow the route's subject; it does
not need to reuse the Indian instrument visual language.

```console
$ chordrift artwork render \
    --background artwork/routing/NEW_ROUTE_BACKGROUND.png \
    --title "Route — NEW ROUTE" \
    --output artwork/routing/NEW_ROUTE_SPOTIFY.png

$ chordrift routes create \
    --account personal \
    --name "NEW ROUTE" \
    --description "A concise description of what belongs here." \
    --background artwork/routing/NEW_ROUTE_BACKGROUND.png \
    --artwork artwork/routing/NEW_ROUTE_SPOTIFY.png
```

## Reconciliation invariant

A non-empty route means pending work. Later review must either:

1. move the track into an existing canonical playlist and record the correction;
2. promote a coherent cohort into a new poetic canonical playlist with its own
   identity and artwork; or
3. leave it in review because the destination remains uncertain.

Chordrift clears route membership only after the selected destination has been
published and verified. Full automatic route consumption is the next v0.1.2
reconciliation slice; the current routing subslice safely captures and
publishes the corrective inboxes without prematurely clearing them.

## Add a private classification dimension

The complete column glossary, recommended vocabulary, examples, and future UI
token behavior live in [Classify tracks with user dimensions](CLASSIFICATION_DIMENSIONS.md).

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

For a larger review, export one or more playlists into one deduplicated CSV:

```console
$ chordrift classify export \
    --playlist "Monsoon Cinema" \
    --playlist "Route — South Indian" \
    --playlist "Route — North Indian" \
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
