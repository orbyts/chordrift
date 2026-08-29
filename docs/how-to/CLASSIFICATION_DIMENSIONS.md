# Classify tracks with user dimensions

Applies to v0.2.0. These revisioned facts remain durable input to its
provider-neutral collection and recipe model.

Chordrift keeps three kinds of knowledge separate:

1. the base acoustic model describes how a recording sounds;
2. public or model-inferred facts retain their own source and confidence; and
3. user dimensions describe how one account understands and organizes a track.

User dimensions are not corrections to Spotify, MusicBrainz, or the acoustic
model. They are private, revisioned facts. Every set or clear operation retains
its reason and prior value in Neon.

## CSV columns

Do not edit identity or inferred-evidence columns:

| Column | Rule |
| --- | --- |
| `schema_version` | Leave unchanged. |
| `spotify_id` | Leave unchanged; this is the stable import key. |
| `title`, `artists`, `album` | Read-only review context. |
| `inferred_release_country` | Read-only external/model evidence; blank means unknown. |
| `inferred_release_language` | Read-only release-title language, not necessarily sung language. |

Edit these workflow and user columns:

| Column | Meaning | Shape |
| --- | --- | --- |
| `action` | Blank makes no change; `set` replaces the active user dimensions; `clear` removes them while retaining history. | `set`, `clear`, or blank |
| `user_collection` | One broad library partition. It should not be a genre, artist, decade, or playlist name. | One slug |
| `user_regions` | Cultural/geographic context of this recording—not artist nationality or release market. | Zero or more `;`-separated slugs |
| `user_traditions` | Musical form or tradition. | Zero or more `;`-separated slugs |
| `user_cohorts` | Personal cross-cutting groups that may deserve intentional composition together but do not necessarily sound alike. | Zero or more `;`-separated slugs |
| `user_languages` | Sung/spoken language when known, or `instrumental`; use `multilingual` only when that is the truthful recording-level fact. | Zero or more `;`-separated tags |
| `user_notes` | Optional durable context for a human; notes are not embedding features. | Free text |
| `reason` | Required when `action` is `set` or `clear`; explains why this revision was made. | Free text |

Multiple values use semicolons inside one CSV cell, for example:

```text
ta;te
film;carnatic-classical
ar-rahman-favorites;childhood-favorites
```

Chordrift normalizes dimension values to lowercase hyphenated slugs and removes
duplicates. Excel may quote cells containing punctuation or commas; save the
finished workbook as UTF-8 CSV and do not reorder or rewrite Spotify IDs.

## Recommended vocabulary

Start small. Add a new token only when it has a clear meaning you expect to use
again.

| Dimension | Initial examples |
| --- | --- |
| Collection | `south-asian`, `global` |
| Region | `north-indian`, `south-indian`, `pakistani`, `pan-indian` |
| Tradition | `film`, `classic-hindi-film`, `hindustani-classical`, `carnatic-classical`, `folk`, `devotional`, `score`, `popular`, `dream-pop`, `alternative-rock` |
| Cohort | `ar-rahman-favorites`, `childhood-favorites`, `family-road-trip`, `favorite-film-scores` |
| Language | `hi`, `ta`, `te`, `ml`, `ur`, `en`, `instrumental`, `multilingual` |

`region` and `tradition` answer different questions. `south-indian` is a
region; `carnatic-classical` is a tradition. An artist preference is neither:
use a cohort such as `ar-rahman-favorites`.

## Examples

An A.R. Rahman Tamil film favorite:

```text
action=set
user_collection=south-asian
user_regions=south-indian
user_traditions=film
user_cohorts=ar-rahman-favorites
user_languages=ta
reason=Reviewed during the Monsoon Cinema regional split.
```

Copy/paste CSV template for that case (replace the uppercase identity fields;
leave the two inferred fields untouched when using an existing exported row):

```csv
schema_version,spotify_id,title,artists,album,inferred_release_country,inferred_release_language,action,user_collection,user_regions,user_traditions,user_cohorts,user_languages,user_notes,reason
1,SPOTIFY_TRACK_ID,TRACK_TITLE,A.R. Rahman,ALBUM_TITLE,,,set,south-asian,south-indian,film,ar-rahman-favorites,ta,,Reviewed A.R. Rahman Tamil film favorite during the Monsoon Cinema regional split.
```

When the identity and inferred columns already exist in Excel, copy this
tab-separated header and row starting at the `action` cell:

```text
action	user_collection	user_regions	user_traditions	user_cohorts	user_languages	user_notes	reason
set	south-asian	south-indian	film	ar-rahman-favorites	ta		Reviewed A.R. Rahman Tamil film favorite during the Monsoon Cinema regional split.
```

A 1940s–1950s Hindi cinema recording:

```text
action=set
user_collection=south-asian
user_regions=north-indian
user_traditions=classic-hindi-film
user_cohorts=
user_languages=hi
user_notes=Hindi cinema from the 1940s–1950s.
reason=Reviewed as classic Hindi film music.
```

A Western outlier that should return to sound-based clustering:

```text
action=set
user_collection=global
user_regions=
user_traditions=dream-pop
user_cohorts=
user_languages=en
user_notes=Non-South-Asian outlier; let sound clustering choose its destination.
reason=Reviewed during the Monsoon Cinema regional split.
```

It is valid to leave uncertain dimensions blank. Keep unknown values unknown
instead of guessing from names, ISRC prefixes, artist origin, or release market.

## Direct commands and bulk approval

For one track or a handful sharing the same facts, repeat `--spotify-id` and
dimension options as needed:

```console
$ chordrift classify set \
    --spotify-id FIRST_ID --spotify-id SECOND_ID \
    --collection south-asian \
    --region south-indian \
    --tradition film \
    --cohort ar-rahman-favorites \
    --language ta \
    --reason "Reviewed A.R. Rahman Tamil film favorites"
```

For CSV review, `classify import` creates an inert draft. Inspect the printed
count and approve only the exact batch you intended:

```console
$ chordrift classify import --file REVIEW.csv
$ chordrift classify approve --batch BATCH_ID --confirm BATCH_ID
```

## v0.2 native-client token direction

The UI should render each dimension as a namespaced token. A user can drag a
token onto a track or album cover, but the action must stage a preview rather
than silently mutate every track. Album-level application expands into an exact
reviewable batch, shows conflicts and inherited values, then records individual
account-scoped revisions after approval. Cohort tokens should be visually
distinct from acoustic, cultural, language, and externally inferred facts.
