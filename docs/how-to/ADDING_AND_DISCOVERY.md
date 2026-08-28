# Add songs and preserve discovery context

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

The consumer product should perform steps 2–7 in the background and surface
only an understandable proposal when confidence or intent is ambiguous.

## Recommended: run the intake wizard

For the common mixed batch—Liked Songs, Inbox, From Friends, Liked from Radio,
and From Prompts—run:

```console
$ scripts/chordrift-intake-wizard.sh --account personal
```

The wizard deliberately handles independent kinds of work in a safe order:

1. It performs a fresh `sync pull`, creates an immutable baseline plan, and
   separates actual removals from verified Chordrift-managed playlists from
   routine duplicate/provider-drift cleanup. With one exact baseline-plan
   confirmation it records actual removals as reversible Neon exclusions;
   this does not write to Spotify. Routine duplicate removals and existing
   publication work wait until intake coverage is complete.
2. It runs the read-only Rust intake audit over that exact snapshot. The report
   separates tracks already covered by a current managed playlist, active past
   exclusions, approved-but-unpublished assignments, draft suggestions, tracks
   known only from listening history, and genuinely new tracks.

For every active exclusion, choose whether the new intake gesture means
“restore and reconsider” or “keep excluded.” For unresolved tracks, choose a
manual existing destination, an automatic existing-playlist suggestion, or an
explicit exclusion. Automatic placements remain in an editable proposal until
you review the exact destination and approve the complete generation. When a
track has no embedding or no accepted existing-playlist fit, the wizard stays
in the same session and offers manual placement, exclusion, or explicit
deferral; this is a review outcome, not an apply failure.

The wizard can reuse the existing reviewed artwork files only when Chordrift
validates them unchanged against the new proposal. It stops when a new playlist
or genuinely new artwork is required so that naming/artwork design remains a
separate explicit workflow. Provider execution remains phase-separated:
publish, pull/verify, reconcile routine duplicates, pull/verify, then
exact-confirmed destructive intake cleanup. The wizard keeps generating fresh
plans until it reaches zero operations; it no longer stops after an arbitrary
five phases. A repeated phase with the same operation types, exact
playlist/track targets, and payloads is treated as a no-progress cycle and
stops safely.
After Spotify accepts a phase, the helper performs up to four verification
pulls before allowing the next phase. Destructive cleanup also re-proves exact
canonical membership against its own current observation, so an intervening
unchanged pull cannot invalidate otherwise current verification evidence.

Use the read-only form when you only want the classification report:

```console
$ scripts/chordrift-intake-wizard.sh --account personal --review-only
```

`--skip-pull` exists only for the narrow case where you have just completed and
inspected a successful pull yourself. The default fresh pull is the safe path.

## Manual equivalent: separate exclusions, then intake

Start from exact current provider state:

```console
$ chordrift sync pull --account personal
$ chordrift sync plan --account personal
$ chordrift sync plan-show --account personal --details
```

Before changing intake placement, inspect any `exclude_track` operation in the
`reconcile` phase. That operation represents durable exclusion intent, not a
provider removal—the track is already absent from the verified managed
playlist. Record each exact identity reversibly in Neon:

```console
$ chordrift tracks exclude --account personal \
    --spotify-id SPOTIFY_TRACK_ID \
    --reason "Removed from verified managed playlist: PLAYLIST" \
    --confirm SPOTIFY_TRACK_ID
```

Do not apply routine `remove_track` reconciliation yet when the same fresh
inventory contains unresolved intake. Complete the proposal first; readiness
is intentionally whole-library scoped. After coverage is complete, apply
provider phases in order—publish before reconcile—and pull/verify after each.

Now ask Rust to join the fresh intake inventory with Neon intent and history:

```console
$ chordrift intake audit --account personal
```

Spotify is authoritative for current Liked Songs and named-intake membership.
Neon is authoritative for Chordrift assignments, reversible exclusions, and
normalized listening history. The audit performs that join without changing
either system and emits stable redirected TSV for operator scripts.

## Review and place the intake batch manually

After adding a group of tracks to Liked Songs, Inbox, or Re-evaluate, pull once
and inspect the complete unresolved set:

```console
$ chordrift sync pull --account personal
$ chordrift proposals status --account personal
$ chordrift proposals unresolved --account personal --limit 1000
```

An approved proposal is immutable. Prepare a new editable copy before changing
placements:

```console
$ chordrift proposals extend --account personal --min-similarity 1
```

The strict `1` threshold preserves the approved structure without broadly
classifying new tracks. It also replays durable exclusions, manual assignments,
and needs-review decisions, so inspect `proposals unresolved` again afterward.

### Assign private cultural intent first

Spotify metadata does not reliably identify language, region, tradition, or an
A. R. Rahman cohort. Assign tracks you recognize before running general sound
clustering:

```console
$ chordrift proposals list --account personal
$ chordrift proposals assign --account personal \
    --spotify-id FIRST_ID --spotify-id SECOND_ID \
    --playlist PLAYLIST_STABLE_KEY \
    --reason "Reviewed Telugu and A. R. Rahman discoveries"
```

The convenience helper resolves an exact destination display name to its stable
key and also supports moving an existing proposal placement:

```console
$ scripts/chordrift-manual-place.sh --account personal \
    --to "Dakshina Pulse" \
    --spotify-id FIRST_ID --spotify-id SECOND_ID \
    --reason "Reviewed Telugu and A. R. Rahman discoveries"
```

For unresolved tracks that are currently in Inbox, the stricter
`chordrift-intake-move.sh` additionally proves Inbox membership and refuses an
already-resolved track.

### Cluster the reviewed remainder

The established automatic sequence uses direct destination-centroid similarity
of at least `0.05`, then analytical-group consensus of at least 55% with 10
known placed tracks. First run the read-only audit:

```console
$ chordrift proposals placement-audit --account personal
```

If its destination fits and fallback groups look reasonable, run the two
proposal-mutating steps:

```console
$ chordrift proposals centroid-assign --account personal --min-similarity 0.05
$ chordrift proposals consensus-assign --account personal \
    --min-dominance 0.55 --min-evidence 10
```

Those commands update only the editable Neon proposal. When an older durable
`needs_review` decision exists, a later proposal can replay it; converting the
reviewed generated destination into a new explicit assignment supersedes that
older decision. The helper performs the same Rust-owned centroid/consensus
commands and persists their exact destinations for the starting unresolved
set:

```console
$ scripts/chordrift-cluster-unresolved.sh --account personal
```

Default mode is read-only and prints the exact proposal UUID. Apply only after
reviewing that output:

```console
$ scripts/chordrift-cluster-unresolved.sh --account personal \
    --apply --confirm PROPOSAL_GENERATION_UUID
```

The helper reserves unresolved intake, saved/liked, Inbox, and Re-evaluate
tracks by default so private cultural intent is not overwritten. After manually
assigning every special case and reviewing the remaining intake population,
add `--include-intake` to permit ordinary clustering of that remainder.

Finally require zero unresolved inventory, review the entire proposal, approve
its exact generation, bind/approve its artwork batch, and follow the phased
plan/readiness/apply/verify workflow. Approval covers the whole proposal—not
only the tracks handled in the latest command.

## What not to do

- Do not create an arbitrary permanent playlist merely to make Chordrift see a
  new track; use an intake.
- Do not remove a track from its intake before Neon has captured it.
- Do not assume saving alone communicates whether it came from a friend, radio,
  or a prompt.
- Do not run a destructive cleanup before the destination has been verified.
