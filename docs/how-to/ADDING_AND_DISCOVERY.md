# Add songs and preserve discovery context

Use this workflow when you find a song you want Chordrift to remember and
eventually place into the right listening playlist.

## Choose the smallest truthful signal

In Spotify, add the track to one intake playlist:

- `Inbox` for a strong personal discovery;
- `From Friends` for an explicit recommendation;
- `Liked from Radio` for radio or autoplay discovery;
- `From Prompts` for a Spotify prompt-generated discovery.

You may also save the track to Liked Songs. Saving preserves explicit library
interest, but the intake playlist carries the richer “how I found this” signal.
Do not add the same track to several intake playlists unless several origins are
genuinely meaningful.

## Capture it in Neon

```console
$ chordrift sync pull --account personal
$ chordrift tracks inspect --spotify-id SPOTIFY_TRACK_ID
```

The inspection should show the saved or intake provenance. Intake membership
does not force a vibe; embeddings, listening history, semantic metadata, and
your prior corrections inform the destination.

## Placement lifecycle

Current personal workflow:

1. Add the track through Spotify while listening.
2. Pull it into Neon.
3. Generate or revise the Chordrift proposal when ready.
4. Inspect the exact proposed destination and order.
5. Publish through readiness and explicit apply.
6. Pull again and prove the destination exists.
7. Clear the intake only after verified placement.

The consumer product should perform steps 2–7 in the background and surface
only an understandable proposal when confidence or intent is ambiguous.

## What not to do

- Do not create an arbitrary permanent playlist merely to make Chordrift see a
  new track; use an intake.
- Do not remove a track from its intake before Neon has captured it.
- Do not assume saving alone communicates whether it came from a friend, radio,
  or a prompt.
- Do not run a destructive cleanup before the destination has been verified.
