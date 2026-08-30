# External Classification Authority foundation

Status: founding brief for a separate future product/project and Chordrift
dependency, recorded 2026-08-28 and separated from the Chordrift roadmap on
2026-08-29. It is not a Chordrift database module or v0.2.1 slice. Start its own
repository, roadmap, release sequence, and task before implementation. This
brief does not authorize creation of that project or Neon store, collection of
provider audio, cross-account data sharing, a live migration, or a production
service deployment.

## Product objective

The Classification Authority should become exceptionally reliable at describing
recordings from lawful evidence. Chordrift is one future consumer. Providers
remain the source of catalog access, playback, and new discoveries; Chordrift
combines returned classification evidence with private listener context to
split, combine, sequence, and refresh music for dancing, calming down, working,
coding, rediscovery, or a specific cultural/linguistic tradition.

This is classification in a broad sense. A recording does not have one correct
playlist label. It has several independently supported dimensions, including:

- recording identity, release/version, artists, language, region, tradition,
  era, instrumentation, tempo, energy, mood, and social/function context;
- reusable acoustic and semantic neighborhoods;
- shared but revisable classifications with provenance and confidence; and
- private listener meaning, preference, familiarity, exclusions, corrections,
  and situation-specific suitability.

The result should be a multidimensional evidence profile, not one opaque genre
or vibe assignment.

The complete inventory of private behavior, lifecycle and preference signals;
shared candidate evidence; negative evidence; promotion boundaries; and
evaluation obligations lives in
[`LEARNING_SIGNAL_TAXONOMY.md`](LEARNING_SIGNAL_TAXONOMY.md). Its separation is
normative: shared classification describes the recording, Chordrift's private
model describes the listener's relationship to it, and placement/recipe policy
decides where it should appear now.

The authority never turns that profile directly into a fixed playlist
taxonomy. Chordrift may combine or split the same rich claims differently for
each account according to collection depth and explicit preference. A large
single-artist catalog can justify several artist-specific listening surfaces;
a small catalog should remain inside broader collections. Era, region,
language, classical tradition, film/non-film context, and artist identity stay
available as independent evidence even when the current account chooses one
combined playlist. Cadence and recipe policy order the selected surface; they
do not alter the underlying classification claims.

The target explanation is concrete:

```text
Title text: likely Tamil/transliterated Tamil
Album/film context: South Indian cinema
A. R. Rahman: multilingual composer
Performers: language distributions learned from their catalogs
Nearby reviewed examples: predominantly Tamil
Reviewed classification: Tamil
Confidence: 0.93
```

Each line is independently sourced evidence. Title language is not silently
promoted to vocal language; a multilingual artist prior is supporting evidence,
not proof; and placement in a mixed South Indian collection may teach private
eligibility without fabricating or contributing a Tamil factual label.

The authority is a learned classifier, not an exhaustive catalog of final
answers. It is not required to store every recording or a permanent
classification row for every recording. It retains compact, reviewable
knowledge that improves generalization: representative examples, taxonomies,
source facts, artist/catalog priors, disagreements, evaluation cases, and
versioned model artifacts. Given an unseen recording, the selected release
returns ranked claims, calibrated confidence, alternatives, supporting
evidence, and an explicit unknown/conflict state. Weak or closely competing
evidence must cause abstention or review rather than a forced label.

## Low-cost first deployment in the separate project

“Authority” is a logical ownership and versioning boundary, not a requirement
for an always-running service. The first useful deployment has zero idle
inference cost:

```text
Separate Neon knowledge project + developer Classification Lab
                         │ publish
                         ▼
Immutable knowledge/model package
                         │ load on demand
                         ▼
In-process Rust classification runner
                         │ cache exact report
                         ▼
Private consumer cache (Chordrift is one consumer)
```

The authoring store contains taxonomy, selective lawful metadata, reviewed
learning examples,
corrections, provenance, candidates, and evaluations. The intended development
deployment is a separate Storexa-backed Neon project so shared knowledge has
independent credentials, migrations, backups, retention, and lifecycle from the
private account ledger. It can autosuspend and does not need continuous
availability, keeping current development infrastructure effectively free while
remaining subject to measured project/storage/compute limits. Published
packages contain only immutable,
checksummed taxonomy, feature-pipeline, existing-model/adapter, artist-prior,
evaluation, and compatibility artifacts. Chordrift loads a package only for an
unknown or stale track, caches the report privately by
track/input/shared-release fingerprint, and exits. Spins normally consume
cached classifications. A shared result cache may later improve throughput,
but it is disposable infrastructure rather than classification authority.

The account's private overlay remains separate from the shared package. It
contains private corrections, placement evidence, preferences, exclusions, and
policy revisions used downstream for account-specific ranking and playlist
composition. Those signals do not train or alter a shared release unless the
person explicitly contributes a bounded factual claim. Activating a new shared
release changes one account-scoped pointer; rollback selects the previous
immutable release. No exhaustive catalog embedding or always-on vector service
is required.

The ordinary query boundary is deliberately narrow:

```text
Chordrift observes a Like or unknown recording (private trigger)
             │
             ├── stays private: account, Like, plays, playlists, behavior
             │
             └── query: recording identity + permitted catalog metadata
                                      │
                                      ▼
                         Selected shared model release
                                      │
                                      ▼
             ranked claims · confidence · alternatives · evidence
                         unknown/conflict · release identity
                                      │
                                      ▼
                    private exact-fingerprint report cache
```

If usage later justifies it, the identical classification contract may run in a
scale-to-zero job, queued shared worker, or always-on service with a shared
cache. Deployment changes must not change the report semantics.

### Physical data boundary

The two Neon projects serve different authorities:

| Store | May contain | Must not contain |
| --- | --- | --- |
| Shared knowledge project | Selective canonical identities and source facts, taxonomy, lawful source/license provenance, representative reviewed examples, artist/catalog priors, explicitly scoped contribution events, disagreements, model/artifact manifests, shared evaluations, optional rebuildable indexes | An exhaustive song-by-song answer catalog, private libraries, playlist names, listening history, exclusions, private corrections, personal model overlays, provider credentials |
| Private account project | Provider inventory, listening evidence, personal labels/corrections, placement and removal evidence, exclusions, personal releases, active release, cached account classification reports | Another person's private evidence or unreviewed claims presented as shared truth |

Storexa supplies the same typed connection, health, migration, and recovery
machinery to both Rust-owned stores. The browser never connects to either.
During initial development the local Rust Lab is the only writer to the shared
project. Friends later contribute through authenticated application commands;
they never receive Neon credentials. Before hosted identity exists, any friend
annotation must arrive as an explicit reviewable import and cannot become a
promoted shared claim automatically.

A contribution contains only the claim and evidence the contributor chose to
share, with contributor scope, consent/provenance, time, and withdrawal state.
It does not implicitly disclose their library, playlists, listening behavior,
or private classifier. Conflicting contributions remain visible until a
review/evaluation process promotes a shared conclusion.

### Placement gestures as candidate learning evidence

A direct provider move into a reviewed Chordrift destination first records
private account placement intent. For example, moving a track into Celluloid
Mehfil supports private eligibility for that surface; it does not by itself
prove a globally reusable `classic-hindi-film` fact. Repeated local moves may
improve the private overlay and identify tracks that need Classification Lab
review.

Only a separate explicit contribution action may minimize one such gesture
into candidate shared evidence. That contribution carries recording identity,
the bounded proposed dimension/value, evidence type (`reviewed-placement`),
taxonomy and model versions, confidence, consent provenance, and a revocable
contribution identifier. It must omit account identity, provider playlist
names, listening history, play counts, exclusions, neighboring private tracks,
and unrelated behavior. The Classification Authority evaluates the candidate
against lawful metadata, existing examples, conflicts, and held-out tests
before it can influence a published shared release. Chordrift never streams raw
playlist activity into shared training automatically.

### Provider recommendations are optional affinity evidence

Spotify's client may show recommendations “based on what's in this playlist.”
That proves the provider has useful internal affinity models, but the visible
client surface is not a public playlist-recommendations contract. Spotify also
stopped granting its general Recommendations, Audio Features, and Audio
Analysis Web API functionality to new and development-mode applications in
November 2024. Chordrift must not scrape the client or depend on grandfathered
access as a product foundation.

The provider adapter may later expose a licensed recommendation or similarity
capability when a provider offers one under stable commercial terms. Such a
result is a ranked, provenance-bearing affinity signal—not canonical genre,
language, region, era, or playlist membership. The Classification Authority
may consume only evidence whose license permits that use. Current Spotify API
policy explicitly forbids using Spotify Platform or Spotify Content to train a
machine-learning or AI model.

A listener explicitly selecting **Add** beside a Spotify recommendation is a
different event: Chordrift observes the resulting membership as the listener's
private placement decision. Alpha.6 records that decision without copying the
unselected recommendation list. It can inform the private overlay and a later
explicit, minimized contribution path, subject to the boundaries above.

References: [Spotify Web API changes, 2024-11-27](https://developer.spotify.com/blog/2024-11-27-changes-to-the-web-api),
[Spotify playlist API policy](https://developer.spotify.com/documentation/web-api/reference/get-playlist),
and [Spotify development access update, 2026-02-06](https://developer.spotify.com/blog/2026-02-06-update-on-developer-access-and-platform-security).

## One logical knowledge authority, several physical stores

The intended “master” is a versioned classification knowledge authority—not a
publicly writable vector database. Its contract presents one authoritative
view while its implementation may be an offline package, an in-process runner,
or later several specialized hosted stores:

```text
Lawful catalog, metadata, and model inputs
                    │
                    ▼
Shared learning knowledge
  examples · taxonomy · priors · explicit facts · provenance · conflicts
                    │
          ┌─────────┴─────────┐
          ▼                   ▼
Model/artifact registry   Shared vector indexes
versions · licenses       acoustic · semantic · fused
          └─────────┬─────────┘
                    ▼
      Versioned classification boundary
                    │
          ┌─────────┴─────────┐
          ▼                   ▼
Private account overlay   Context/recipe request
preferences · corrections mood · activity · constraints
          └─────────┬─────────┘
                    ▼
Explainable account-specific ranking and playlist composition
```

The relational knowledge store remains authoritative for the selected
identities and facts it does retain, plus examples, licenses, provenance,
confidence, model versions, and review history. It is not the authoritative
answer only because a recording has a row. Vector indexes are optional,
rebuildable retrieval structures keyed by example or canonical recording,
modality, model revision, input revision, and dimensions. Model weights and
larger artifacts belong in a versioned artifact registry/object store. When
deployed remotely, other applications consume an authenticated API rather than
receiving direct access to operational tables or another person's vectors. The
local developer Lab calls the same Rust boundary in process and never gives the
browser database or provider credentials.

## Representation layers

### 1. Shared reviewed facts and examples

Selected evidence-backed facts and representative examples—such as recording
identity, language, artist association, release geography, tradition, era, and
instrumentation—remain explicit and queryable when they add learning or audit
value. Every claim carries source, scope, time, parser/model revision,
confidence, and conflict state. Unknown and disputed values stay visible. The
store may deliberately omit an ordinary recording that the released classifier
can already handle reliably.

These facts supply hard or explainable boundaries that geometric similarity
alone cannot guarantee. “Hindi,” “Tamil,” “1970s Hindi film music,” and
“French-language” must not be guessed solely from a nearby vector or title.

### 2. Shared base representations

A recording may have separately versioned acoustic, lyrical/text-semantic,
catalog-semantic, and relationship embeddings. Chordrift may publish a fused
shared representation, but it must retain its component identities so a result
can explain which evidence contributed and degrade honestly when one modality
is unavailable.

No vector is updated in place. A new model, lawful input, taxonomy, or fusion
method creates a new generation that can be evaluated and rolled back.

### 3. Shared reviewed knowledge

Some classifications can become reusable shared knowledge when they are backed
by licensed/public evidence, expert review, or sufficiently consistent
consented observations. Agreement raises confidence; disagreement is retained
as scoped evidence instead of being averaged into a false universal answer.

Shared learning must never expose a user's library, playlist names, listening
history, or private classifications. Consent, contribution withdrawal,
aggregation thresholds, abuse resistance, and privacy review are prerequisites,
not cleanup work after launch.

### 4. Private account overlay

Listener-specific meaning remains in the account boundary: preference,
familiarity, personal eras, private cultural groupings, corrections, pinned or
excluded tracks, collection membership, and context suitability. It may be
represented as facts, learned ranking parameters, or private vectors, but it
does not mutate the shared canonical representation.

The private layer nudges or constrains a shared classification. It can say that
a broadly energetic track works for this listener's party set, or that two
acoustically similar tracks belong to different personal contexts, without
claiming either judgment is globally canonical.

### 5. Request-time context

“Dance,” “calm,” “coding,” and similar requests are not permanent track labels.
They are versioned recipe/context profiles evaluated against shared evidence
and the private overlay. Composition also considers repetition, freshness,
familiarity, duration, exclusions, and narrative ordering. Chordrift therefore
answers “what fits this listener in this situation?” rather than pretending to
discover music the provider has not supplied.

## Learning strategy

Early improvement should preserve a stable pretrained base and learn smaller,
auditable components: calibration, projections, classifiers, or ranking
adapters over reviewed examples. The goal is generalization to recordings that
have never appeared in the shared store. Useful shared learning evidence
includes explicit dimension labels, comparisons, boundary corrections, source
facts, and curator-reviewed examples. Account placements and passive behavior
remain private placement evidence; they are neither shared training labels nor
unambiguous factual classifications without an explicit contribution and
review boundary.

Training a new music foundation model is a later option, not an assumption.
It requires sufficient lawful data rights, commercially compatible model
weights, representative multilingual evaluation data, privacy controls, and a
measured advantage over adapters around an existing base. Every promoted model
must pass versioned evaluation suites covering regional and linguistic
minorities, calibration, unknown handling, account isolation, regressions, and
explanation quality.

## Lawful-input boundary

Chordrift must not download, scrape, record, or reconstruct Spotify or another
provider's audio when the provider does not authorize it. Possible inputs must
come through proper legal channels: licensed catalog/audio agreements,
commercially compatible public datasets and models, provider APIs within their
terms, user-owned DRM-free audio with explicit scope, or independently licensed
metadata and annotations.

Rights review applies not only to raw audio but also to model weights, training,
derived vectors, retention, cross-user reuse, export, and commercial service
use. If rights are insufficient, the modality is unavailable; Chordrift uses
the remaining evidence and reports lower confidence rather than inventing it.

## Dependency boundary with Chordrift

The existing database-v2 and v0.2 product schema remain Chordrift's private
account ledger and orchestration authority. Current account embeddings remain
rebuildable Chordrift caches. The separate project defines and versions its own
classification request/report contract, shared authoring ledger, model package
runner, release lifecycle, evaluation, and developer Lab. Chordrift integrates
only after a compatible dependency release exists; it stores its selected
dependency release and exact private response cache without absorbing the
authority's implementation or database.

A Chordrift integration may use permitted catalog metadata as inference input
only after its own additive migration and release gates pass. It may not
silently copy account evidence into shared training data. Multi-contributor
learning and remote inference remain work for the separate project requiring
hosted identity/authorization, privacy and consent, canonical cross-provider
identity, lawful data/model sourcing, and explicit multilingual evaluation.
