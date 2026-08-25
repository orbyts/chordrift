# How to use Chordrift

This is the user-facing entry point for Chordrift. Start with the task you want
to accomplish; use the comprehensive [CLI command reference](reference/CLI_COMMANDS.md)
only when you need every option or an internal diagnostic command.

Chordrift treats Neon as the durable ledger and Spotify as the familiar
listening surface. A normal pull observes Spotify changes, but ambiguous intent
is staged for inspection rather than silently guessed.

## Everyday actions

| I want to… | Guide | Spotify action |
| --- | --- | --- |
| Add or discover a song | [Add songs and preserve discovery context](how-to/ADDING_AND_DISCOVERY.md) | Like/Save it; use a named intake only for a richer signal. |
| Inventory or retire saved albums | [Saved albums and album cleanup](how-to/SAVED_ALBUMS.md) | Archive-only retirement keeps immutable album and track history. |
| Stop hearing a song | [Delete or exclude a track safely](how-to/DELETING_AND_EXCLUDING.md) | Remove it from its verified Chordrift playlist, then reconcile. |
| Keep a song but reject its current vibe | [Route and reclassify a track](how-to/ROUTING_AND_RECLASSIFYING.md) | Add it to a `Route — …` corrective inbox. |
| Add private region, tradition, language, or cohort facts | [Classify tracks with user dimensions](how-to/CLASSIFICATION_DIMENSIONS.md) | Review one track/a small group directly, or approve a CSV batch. |
| Bring Neon up to date | [Synchronize and prove convergence](how-to/SYNC_AND_CONVERGENCE.md) | Run a pull after provider changes. |
| Understand how a future product infers intent | [Platform interaction model](design/PLATFORM_INTENT_MODEL.md) | Keep using Spotify; Chordrift interprets bounded changes. |
| Understand account isolation and provider-neutrality work | [Account isolation and provider boundaries](design/ACCOUNT_AND_PROVIDER_BOUNDARIES.md) | Current personal facts are account-scoped; a full adapter audit precedes another live provider. |

## The short everyday loop

After making a change in Spotify:

```console
$ chordrift sync pull --account personal
```

Then inspect one track when you want to understand its current state:

```console
$ chordrift tracks inspect --name "Do Your Best" --artist "John Maus"
$ chordrift tracks inspect --spotify-id SPOTIFY_TRACK_ID
```

If the change requires a Chordrift write or durable interpretation, create and
inspect a plan:

```console
$ chordrift sync plan --account personal
$ chordrift sync plan-show --account personal --details
```

Never apply a plan merely because it exists. Confirm that its operations match
your intent, then follow the readiness and apply sequence in
[Synchronize and prove convergence](how-to/SYNC_AND_CONVERGENCE.md).

## Semantic playlists used for capture

Use these playlists to tell Chordrift why a newly encountered track matters:

| Playlist | Meaning |
| --- | --- |
| `Inbox` | Strong recent personal discovery. |
| `From Friends` | Explicit recommendation from someone you know. |
| `Liked from Radio` | Discovery from radio or autoplay. |
| `From Prompts` | Discovery from a Spotify prompt-generated playlist. |

Use `Route — …` playlists differently: they mean “keep this track, but its
current Chordrift destination needs correction.” Routes are temporary inboxes
whose steady state is empty after verified reassignment.

Spotify's Like button is the primary lightweight intake action. It means
“keep and classify.” `Inbox` is the stronger high-interest variant; the other
named intakes carry source provenance. For Suhail's opt-in policy, Liked Songs
is cleared only after verified Chordrift placement, just like an intake queue.

## Important distinctions

- Removing a track from a verified Chordrift playlist can stage an exclusion.
- Removing it only from Liked Songs means “unsave,” not necessarily “forget.”
- Adding it to a route means “keep and reclassify,” not “exclude.”
- Removing it from a protected user-managed playlist changes that playlist but
  does not automatically teach a global preference.
- Exclusion removes a track from active Chordrift listening surfaces; it does
  not erase listening history, provenance, or recoverability from Neon.

## Setup and complete reference

For installation, OAuth, archive recovery, embeddings, clustering, proposal
generation, artwork, retirement, bookmarks, enrichment, and every CLI leaf
command, see the [CLI command reference](reference/CLI_COMMANDS.md).

The roadmap and current implementation state are recorded in
[ROADMAP.md](../ROADMAP.md) and [CODEX_HANDOFF.md](../CODEX_HANDOFF.md).
