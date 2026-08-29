# Classification knowledge foundation

Status: active architecture direction, recorded 2026-08-28 and reconciled for
the low-cost v0.2.1 personal implementation on 2026-08-29. It does not authorize
collection of provider audio, cross-account data sharing, a live migration, or
a production service deployment.

## Product objective

Chordrift should become exceptionally reliable at organizing music a person
already knows and likes. Providers remain the source of catalog access,
playback, and new discoveries. Chordrift's job is to understand the listener's
existing musical world well enough to find useful, explainable ways to split,
combine, sequence, and refresh it for a context such as dancing, calming down,
working, coding, rediscovery, or a specific cultural/linguistic tradition.

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

For the personal implementation, the target explanation is concrete:

```text
Title text: likely Tamil/transliterated Tamil
Album/film context: South Indian cinema
A. R. Rahman: multilingual composer
Performers: language distributions learned from their catalogs
Nearby accepted examples: predominantly Tamil
Your accepted classification: Tamil
Confidence: 0.93
```

Each line is independently sourced evidence. Title language is not silently
promoted to vocal language; a multilingual artist prior is supporting evidence,
not proof; and placement in a mixed South Indian collection may teach personal
eligibility without fabricating a Tamil factual label.

## Immediate low-cost deployment

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
Private Chordrift account ledger
```

The authoring store contains taxonomy, lawful metadata, training examples,
corrections, provenance, candidates, and evaluations. The intended development
deployment is a separate Storexa-backed Neon project so shared knowledge has
independent credentials, migrations, backups, retention, and lifecycle from the
private account ledger. It can autosuspend and does not need continuous
availability, keeping current development infrastructure effectively free while
remaining subject to measured project/storage/compute limits. Published
packages contain only immutable,
checksummed taxonomy, feature-pipeline, existing-model/adapter, artist-prior,
evaluation, and compatibility artifacts. Chordrift loads a package only for an
unknown or stale track, caches the report by track/input/model/personal-release
fingerprint, and exits. Spins normally consume cached classifications.

The account's personal overlay remains separate from the shared base package.
It contains accepted corrections, small learned adapters/prototypes, placement
evidence, policy revision, and evaluation. Activating a new release changes one
account-scoped pointer; rollback selects the previous immutable release. No
global catalog embedding or always-on vector service is required.

If usage later justifies it, the identical classification contract may run in a
scale-to-zero job, queued shared worker, or always-on service with a shared
cache. Deployment changes must not change the report semantics.

### Physical data boundary

The two Neon projects serve different authorities:

| Store | May contain | Must not contain |
| --- | --- | --- |
| Shared knowledge project | Canonical identities and facts, taxonomy, lawful source/license provenance, artist/catalog priors, explicitly scoped contribution events, reviewed shared claims, model/artifact manifests, shared evaluations, optional rebuildable vector indexes | Private libraries, playlist names, listening history, exclusions, private corrections, personal model overlays, provider credentials |
| Private account project | Provider inventory, listening evidence, personal labels/corrections, placement and removal evidence, exclusions, personal releases, active release, cached account classification reports | Another person's private evidence or unreviewed claims presented as shared truth |

Storexa supplies the same typed connection, health, migration, and recovery
machinery to both Rust-owned stores. The browser never connects to either.
During personal development the local Rust Lab is the only writer to the shared
project. Friends later contribute through authenticated application commands;
they never receive Neon credentials. Before hosted identity exists, any friend
annotation must arrive as an explicit reviewable import and cannot become a
promoted shared claim automatically.

A contribution contains only the claim and evidence the contributor chose to
share, with contributor scope, consent/provenance, time, and withdrawal state.
It does not implicitly disclose their library, playlists, listening behavior,
or private classifier. Conflicting contributions remain visible until a
review/evaluation process promotes a shared conclusion.

## One logical knowledge authority, several physical stores

The intended “master” is a versioned classification knowledge authority—not a
publicly writable vector database. Its contract presents one authoritative
view while its implementation may be an offline package, an in-process runner,
or later several specialized hosted stores:

```text
Lawful catalog, metadata, and model inputs
                    │
                    ▼
Shared canonical knowledge
  identities · explicit facts · provenance · confidence · conflicts
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

The relational knowledge store remains authoritative for identities, facts,
licenses, provenance, confidence, model versions, and review history. Vector
indexes are rebuildable retrieval structures keyed by canonical recording,
modality, model revision, input revision, and dimensions. Model weights and
larger artifacts belong in a versioned artifact registry/object store. When
deployed remotely, other applications consume an authenticated API rather than
receiving direct access to operational tables or another person's vectors. The
local developer Lab calls the same Rust boundary in process and never gives the
browser database or provider credentials.

## Representation layers

### 1. Shared canonical facts

Evidence-backed facts such as recording identity, language, artist association,
release geography, tradition, era, and instrumentation remain explicit and
queryable. Every claim carries source, scope, time, parser/model revision,
confidence, and conflict state. Unknown and disputed values stay visible.

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
adapters over reviewed examples. Useful training evidence includes explicit
dimension labels, accepted/rejected placements, comparisons, boundary
corrections, and context-specific choices. Passive behavior is weaker evidence
and must not be treated as an unambiguous label.

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

## Relationship to current Chordrift

The existing database-v2 and v0.2 product schema remain the private account
ledger and orchestration authority. Current embeddings are rebuildable caches
inside that deployment. v0.2.1 first introduces the classification report,
private evidence/release ledger, on-demand package runner, and developer Lab
without extracting a shared service or distributing private data.

The local implementation may reuse the live account's existing evidence only
after its additive migration and release gates pass. Shared cross-account
learning and remote inference remain later work requiring hosted
identity/authorization, privacy and consent, canonical cross-provider identity,
lawful data/model sourcing, and explicit multilingual evaluation.
