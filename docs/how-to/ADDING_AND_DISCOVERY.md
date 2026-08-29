# Add songs and preserve discovery context

> **Alpha.4 limitation:** moving an existing managed track directly between
> managed Spotify playlists is supported. Adding a previously unknown track
> directly to a managed playlist is reserved for A021-07 / alpha.5. Until that
> release, use Liked Songs or a named intake playlist for genuinely new tracks;
> do not approve a plan that describes the new managed membership as drift to
> remove.

These capture semantics and the V020-11R capability-checked helpers apply to
v0.2.0. The `v0.1.4` tag remains the exact historical reference.

Use this workflow when you find a song you want Chordrift to remember and
eventually place into the right listening playlist.

## Choose the smallest truthful signal

The default and easiest intake action is Spotify's Like/Save button. It means:
“keep this track and let Chordrift classify it.” A normal pull records the
track and its saved timestamp in Neon.

Use a named intake playlist only when you want to provide a stronger or more
specific signal:

- `Inbox` for a strong personal discovery;
- `From Friends` for an explicit recommendation;
- `Liked from Radio` for radio or autoplay discovery;
- `From Prompts` for a Spotify prompt-generated discovery.

`Inbox` also means higher current personal interest than a plain Like. The
other intake playlists retain discovery provenance that Like alone cannot.
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

For an account configured with the opt-in Liked Songs cleanup policy, step 7
also removes the track from Liked Songs. Neon retains the original save event,
and the removal is planned as a destructive cleanup operation only after the
canonical destination is verified:

```console
$ chordrift spotify library-policy --account personal \
    --liked-songs clear-after-verified-assignment
```

The safe product default is `preserve`; changing policy does not immediately
write to Spotify.

The later hosted/native product should perform steps 2–7 in the background and
surface only an understandable proposal when confidence or intent is ambiguous.

## Review ordinary additions and discovery

For current Liked Songs, Inbox, From Friends, Liked from Radio, From Prompts,
managed-playlist edits, and direct reclassification moves, use the single
capability-checked wizard:

```console
$ scripts/chordrift-maintain.sh --account personal
```

It performs one initial observation, requires `plan_origin: maintenance`, and
uses the read-only Rust audit internally:

```console
$ chordrift intake audit --account personal
```

The audit labels exact provider identities as `already_covered`,
`previously_excluded`, `assigned_approved`, `suggested_in_draft`,
`known_from_history`, or `genuinely_new`. It makes no provider request and no
database write. Use `--review-only` when that report is the desired endpoint.

The interactive path asks for an existing destination only when placement is
ambiguous, then shows one exact net-change summary and asks once. It refuses new
playlist/artwork design, retirement, unrelated work, and non-maintenance plans.
Lower-level proposal and clustering commands remain developer diagnostics.

## What not to do

- Do not create an arbitrary permanent playlist merely to make Chordrift see a
  new track; use an intake.
- Do not remove a track from its intake before Neon has captured it.
- Do not assume saving alone communicates whether it came from a friend, radio,
  or a prompt.
- Do not run a destructive cleanup before the destination has been verified.
