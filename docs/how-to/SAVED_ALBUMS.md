# Saved albums and album cleanup

Spotify saved albums are distinct from playlists and Liked Songs. Chordrift
inventories them as a separate immutable surface, including the ordered tracks
inside every album. Album-only tracks do not automatically enter a canonical
playlist and do not block ordinary playlist readiness.

## Inventory before changing Spotify

```console
$ chordrift sync pull --account personal
$ chordrift albums list --account personal
$ chordrift albums audit --account personal
```

Inspect one album by exact title or stable Spotify ID:

```console
$ chordrift albums tracks --account personal --name "ALBUM TITLE"
$ chordrift albums tracks --account personal --spotify-id SPOTIFY_ALBUM_ID
```

Every track has one review disposition:

- `preserved`: already in Liked Songs or a current playlist;
- `excluded`: explicitly recorded as unwanted;
- `review`: neither preserved nor excluded yet.

Do not unsave an album while any of its tracks remain `review`. Add wanted
tracks to Inbox or Like them, pull again, and explicitly exclude only tracks
you genuinely do not want retained. Neon keeps every historical album snapshot
even after a future cleanup.

## Account policy

The product default is safe and non-mutating:

```console
$ chordrift albums policy --account personal --mode preserve
```

Suhail's intended personal cleanup policy is:

```console
$ chordrift albums policy --account personal --mode review-then-unsave
```

This records intent only. v0.1.2 will not propose an album unsave until every
track has an explicit disposition, and applying that future cleanup will still
require exact readiness confirmation plus `--allow-destructive`.
