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
