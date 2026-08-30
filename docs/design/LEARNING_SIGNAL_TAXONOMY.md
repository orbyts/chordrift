# Learning-signal taxonomy

Status: founding input for the separate future Classification Authority and
Chordrift's private listener model, recorded 2026-08-30. This is a design
contract, not authorization to collect, export, aggregate, or train on provider
or personal data.

## Keep three questions separate

| Plane | Question | Owner | Example |
| --- | --- | --- | --- |
| Shared classification | What is this recording like? | Classification Authority | likely Tamil vocals; Indian film context; energetic; confidence 0.93 |
| Private listener model | What does it mean to this person? | Chordrift account boundary | strong favorite; recently rediscovered; useful for coding |
| Placement and recipe policy | Where should it appear now? | Chordrift | keep in Dakshina Pulse; eligible for Dance Spin; resurface on request |

Classification is not playlist destination. A recording can have several true
classifications and several listening uses. Co-membership in a mixed playlist
is not proof of shared genre, language, region, or culture.

## Evidence envelope

Retain observations before deriving features. Each signal needs canonical
recording/release identity and identity confidence; private account/provider
scope; observed and effective time; source and destination; before/after state;
track occurrence and position; user/provider/Chordrift authorship; provenance,
license, consent, and retention scope; derivation/model/taxonomy version; and
uncertainty, conflict, supersession, and withdrawal state.

An event is evidence, not ground truth. Derived features are rebuildable; raw
observations and explicit decisions remain auditable.

## Private user-level signals

These refine preference, familiarity, lifecycle, and context for one account.
They stay private by default.

### Explicit intent

| Signal | Useful interpretation | Caveat |
| --- | --- | --- |
| Like/save and Unlike/unsave | Current interest or intentional intake | A Like may be a bookmark; an Unlike is not global exclusion |
| Direct add to a managed collection | Strong current placement | Private eligibility is not universal classification |
| Move between collections | Reclassification or changed personal meaning | Learn the removal and destination together |
| Removal from a managed collection | Negative placement and reversible exclusion | It may mean wrong context rather than dislike |
| Manual label/factual correction | Strong scoped claim | Shared use requires separate consented contribution |
| Accept, reject, or edit a suggestion | Direct feedback on an exact proposal | Retain the proposal and alternatives |
| Pin, protect, exclude, restore, or forget | Explicit policy stronger than passive behavior | Preserve track/surface/recipe/account scope |
| Create, rename, describe, or retire a playlist | Declared personal taxonomy | Names may be poetic, ironic, or broad |
| Manual reorder or move-to-top | Cadence and resurfacing intent | Position is not classification |
| Keep/clear an already-represented Like | Saved-state preference | Separate from destination and order |

### Listening and lifecycle evidence

- play count plus lifetime and rolling-window rotation;
- first heard, first saved, first placed, last played, and time since play;
- completion ratio, completed plays, early/late skips, seeks, replays, and
  immediate repeats;
- session recurrence, consecutive-day return, and return after dormancy;
- direct selection, search, queue insertion, playlist playback, radio,
  autoplay, recommendation, or background playback source;
- time of day, day of week, season, and explicitly known activity context; and
- novelty/familiarity balance and the amount of evidence available.

Autoplay, shared devices, interrupted sessions, missing history, and provider
retention limits can distort these signals. One skip is not dislike; dormancy
is not rejection. Repeated contextual patterns are stronger than isolated
events, and sensitive context must be minimized and transparent.

### Rediscovery

A new Like for a track already present in a managed collection is a
**rediscovered-favorite** candidate. Report its collection and canonical
position. A return after long dormancy, deliberate search/direct selection,
repeat, or completion can strengthen the signal. A move-to-top choice is
explicit resurfacing intent; the Like alone never authorizes reordering.
Removing that Like later changes saved-state intent without deleting the
historical rediscovery observation.

### Collection, social, and provenance evidence

- current and historical collection membership, recurrence across collections,
  neighbors, sections, and occurrence positions;
- source such as Likes, Inbox, radio, search, prompt playlist, import, concert,
  external list, or explicit recommendation;
- appearance in a friend's favorites and whether the listener accepted,
  ignored, moved, removed, or repeatedly played it;
- session co-listening and cross-surface duplication; and
- user edits made after a Chordrift proposal.

Friend provenance is discovery evidence, not proof of the recipient's taste.
Playlist neighborhoods are weighted by declared purpose, homogeneity, size,
and user confirmation. Mixed playlists provide weak classification evidence.

### Private affinities to derive

Maintain independently explainable affinity for recording, artist, contributor,
ensemble, album, label and film; language, region, culture, tradition and era;
genre/subgenre; vocal/instrumental balance and instrumentation; rhythm, tempo,
energy, valence, density, timbre and acoustic/electronic character; mood,
social function and activity suitability; novelty/familiarity and diversity;
explicit constraints; and transition, artist-spacing, repetition, duration and
cadence preferences.

Each affinity carries scope, evidence volume, recency, and uncertainty. A
preference in one context must not become a universal preference.

## Shared Classification Authority signals

The shared authority predicts reusable multidimensional claims about a
recording using only evidence whose provenance and license permit inference,
retention, training, redistribution, and commercial use.

### Strong factual/reviewed evidence

- canonical cross-provider recording, work, release, and version identity;
- licensed/public catalog facts: credits and roles, release date/place, label,
  film/series association, and external authority identifiers;
- lawfully usable language/script evidence, keeping written title language
  separate from sung language;
- artist, performer, composer, lyricist, ensemble, instrument, and catalog
  priors with multilingual and cross-genre uncertainty;
- curator definitions, reviewed examples, boundary cases, counterexamples, and
  explicit negative labels;
- independently licensed acoustic, lyrical, semantic, and relationship
  representations; and
- consented factual corrections with provenance, review, and withdrawal.

### Candidate learned evidence

| Signal | Potential value | Reliability rule |
| --- | --- | --- |
| Placement in a narrowly defined reviewed collection | One bounded category candidate | Mixed/poetic collections are weak; require declared semantics |
| Move out of one category and into another | Boundary correction | Preserve both sides; separate personal fit from fact |
| Pairwise “more like A than B” judgment | Neighborhood and boundary learning | Store dimension, candidates, and confidence |
| Accepted/rejected proposed label | Classifier feedback | Avoid training directly on the model's own suggestion |
| Independent contributor agreement | Coverage/confidence | Require privacy thresholds and independence; popularity is not truth |
| Contributor disagreement | Ambiguity, scope, or taxonomy flaw | Preserve alternatives rather than averaging |
| Co-placement graph | Affinity prior | Never promote alone to language/genre/region/era fact |
| Artist/catalog distribution | Unseen-track prior | Multilingual/collaborative work needs stronger evidence |
| Album/film and collaborator graph | Contextual prior | Releases may contain multiple languages/styles |
| Licensed provider affinity | Neighborhood evidence | Not a canonical label; use only when licensed |
| Model/LLM title-language hypothesis | Sparse/transliterated metadata candidate | Written language is not automatically vocal language |

### Negative and abstention evidence

Retain `not-language`, `not-region`, `not-tradition`, wrong-version, rejected
candidate, and competing-alternative evidence. Explicitly test mixed playlists,
ambiguous transliteration, multilingual lyrics, instrumentals, covers, remixes,
live versions, compilations, incorrect metadata, taxonomy gaps, and temporal
drift. Unknown/conflict/abstain is a valid output and should identify the
smallest useful review question.

## Strength and feedback-loop rules

1. Explicit scoped intent outranks passive behavior for that scope.
2. Repetition from autoplay or one source is not independent agreement.
3. Recency changes current preference, not factual classification.
4. Personal placement can override composition without changing shared truth.
5. Acceptance of a model suggestion is not an independent second vote.
6. Corrections and negative examples survive model releases.
7. Conflicts retain provenance, alternatives, scope, and confidence.
8. Popularity must not erase regional, linguistic, minority, or niche classes.
9. Guard against coordinated poisoning, duplicate contributors, selection bias,
   self-reinforcing predictions, and overrepresentation of highly active users.

## Promotion boundary

```text
private provider/account event
        -> private observation and feature
        -> stays private for account ranking/placement
           OR explicit minimized revocable contribution
        -> bounded candidate claim
        -> provenance, rights, privacy, and abuse checks
        -> review, aggregation, and conflict handling
        -> held-out evaluation
        -> immutable shared model/knowledge release
```

Never stream raw libraries, playlist names, neighbors, play counts, friends, or
listening history into shared learning. A contribution is a deliberately
selected claim, not consent to export its context. Provider-originated content
or metadata cannot enter shared training unless its licenses explicitly permit
that use. Current Spotify Platform policy prohibits using Spotify Platform or
Spotify Content to train an ML or AI model, so API access is not training
permission.

## Minimum useful first release

1. Define a small multidimensional taxonomy with unknown/conflict states.
2. Use independently licensed metadata and a commercially usable base model.
3. Build the Classification Lab for explicit examples, corrections,
   comparisons, and counterexamples.
4. Publish immutable, calibrated, explainable model/knowledge packages.
5. Keep Chordrift's behavioral, lifecycle, and placement overlay private.
6. Add contributions/aggregation only after identity, consent, withdrawal,
   privacy, abuse resistance, rights review, and evaluation exist.

Evaluate each dimension for precision/recall, calibration, coverage,
abstention, disagreement, and regression. Test multilingual and transliterated
titles, minority catalogs, multilingual artists, film/non-film, traditions,
eras, instrumentals, covers/remixes/live versions, mixed playlists, sparse
metadata, unseen artists, cold start, dormant favorites, rediscovery, autoplay,
and incomplete history.

