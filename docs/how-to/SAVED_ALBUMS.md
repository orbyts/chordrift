# Saved albums and album cleanup

Applies to the released v0.2.0 plan/readiness/apply workflow. Saved-album
evidence and preservation policy remain behind the shared application boundary.

Spotify saved albums are distinct from playlists and Liked Songs. Chordrift
inventories them as a separate immutable surface, including the ordered tracks
inside every album. Album-only tracks do not automatically enter a canonical
playlist and do not block ordinary playlist readiness.

## Inventory before changing Spotify

```console
$ chordrift sync pull --account personal
$ chordrift albums list --account personal
$ chordrift albums audit --account personal
$ chordrift albums history --account personal
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

`review` matters for `review-then-unsave`, where every album track needs an
explicit disposition. It does not block the separate `archive-only` workflow:
that mode retires only the saved-album container and retains the complete album
and ordered track inventory in Neon. Album tracks are not forced into playlists.

## Account policy

The product default is safe and non-mutating:

```console
$ chordrift albums policy --account personal --mode preserve
```

For an account that intentionally wants playlists as its active library surface,
retain the inventory but propose retirement of every current album container:

```console
$ chordrift albums policy --account personal --mode archive-only
$ chordrift sync plan --account personal
$ chordrift sync plan-show --account personal --details
```

The policy command is Neon-only. Inspect the immutable plan, approve its exact
plan ID with `sync retirement-approve`, run readiness, and apply only the
`retirement` phase with its exact assessment confirmation and
`--allow-destructive`. Then run `sync pull`; verification succeeds only when
all planned albums are absent from Spotify. `albums history` continues to list
the retired albums and their last inventoried track counts.
