# Daily-driver edge-case ledger

This ledger turns failures found through real Spotify use into durable product
rules and regression obligations. Read it with
`PLATFORM_INTENT_MODEL.md`. It contains no credentials or private library
inventory.

Status: active through the V021-06 private-beta checkpoint, updated 2026-08-31.

## Governing rule

A complete Spotify pull is the newest baseline for user-authority state.
Chordrift records exact user gestures cumulatively and does not write them back,
reverse them, or require duplicate authorization. Ambiguous broader meaning may
remain unresolved. A Chordrift-authored change is separate, must be replanned
against the newest snapshot, and requires explicit authorization before any
provider write.

## Incidents and permanent regressions

The first hosted Spotify reconnect was rejected before consent with
`redirect_uri: Not matching configuration`. The deployed Rust authority
correctly requested
`https://chordrift.suhail.ink/providers/spotify/callback`, while the Spotify
application retained only the local CLI callback. Callback URLs remain server-
derived and exact: clients cannot override them, product login stays separate,
and a rejected request creates no credential or library mutation. The operator
resolution is to allowlist the exact hosted callback alongside any retained
loopback callback and retry **Reconnect Spotify**.

The first hosted disconnect test exposed two provider-lifecycle gaps. A native
same-origin form POST reached the API without the exact `Origin` header required
by the route and returned HTTP 403 before vault revocation. Removing Chordrift
from Spotify's Apps page then invalidated the provider grant out of band, but
the locally active encrypted envelope continued to render as connected after
the next provider operation failed. Disconnect is now a thin same-origin fetch
authenticated by the Chordrift session and a non-simple wrapper header. An
OAuth refresh rejection classified as `invalid_grant`, revoked refresh token,
or equivalent terminal authorization failure immediately revokes the local
envelope, returns `authentication_required`, and causes every client to offer
Reconnect while retaining provider history and Chordrift intent. Because
Spotify does not push third-party revocation events to Chordrift, the UI calls
an unverified local envelope **Authorized** and separately shows when provider
access was last verified; the next explicit provider check is the freshness
boundary.

The same rehearsal then reached Spotify consent successfully but returned to a
still-disabled Reconnect state. The encrypted vault correctly retained revoked
history, but rotation calculated its next generation from only an active row.
After Disconnect there was no active row, so reconnect attempted generation 1
again and collided with immutable generation-1 history. Credential generations
are now monotonic across both active and revoked rows, serialized on the stable
provider account. Reconnect after disconnect creates the next generation,
activates it, and retains every older encrypted audit row.

| Checkpoint | Observed failure | Durable rule | Regression/status |
| --- | --- | --- | --- |
| Pre-alpha / A021-01 | Several UUID-heavy workflows made ordinary cleanup slow and confusing. | One capability-checked maintenance entry point hides internal IDs and asks once only for a provider mutation. | Unified fake-binary workflow suite. Complete. |
| A021-02 | The `Re-evaluate` correction queue added ceremony and could block ordinary work. | Direct Spotify moves are reclassification gestures; the retired empty queue stays absent while Neon history remains. | Retirement and absence checks. Complete. |
| Alpha.1 → alpha.2 | A Dakshina Pulse → Uttara Glow move was treated as drift removal; confirmation could leak into a later plan. Six tracks required recovery. | Recognize the add/remove pair before apply, bind authorization to the exact reviewed phase, and never reuse it after replanning. | Direct-move and confirmation-boundary regressions. Complete. |
| Alpha.2 → alpha.3 | Replaying 615 revisions one row at a time caused a long silent pause. | Maintenance derives current intent with set-based SQL, batches same-destination moves, and reports progress before material work. | Isolated representative rehearsal and performance tests. Complete. |
| Alpha.4 → alpha.5 | A long pause remained between provider observation and the first human-readable change. | Use one bulk labeled plan preview; ordinary review performs no per-track inspection loop. | Fake binary proves zero `tracks inspect` calls. Complete. |
| Alpha.5 → alpha.6 | A track added with Spotify's playlist **Add** action was treated as drift to remove. | A new track added directly to exactly one managed playlist is current placement intent and is preserved without a provider write. | Direct-managed-intake and exclusion-conflict regressions. Complete. |
| Alpha.6 → alpha.7 | A model-only proposal revision failed because the old artwork manifest had 20 covers while the approved system had 25. | Carry the complete unchanged reviewed artwork system across proposal-only revisions and resume interrupted intake. | Artwork carry-forward and resume regressions. Complete. |
| Alpha.7 → alpha.8 | Membership-equal Celluloid Mehfil order drift was classified as an out-of-scope publication reorder. | Provider order is current order intent when exact unique membership is equal; update Neon only and never call provider apply. | Pure reorder fake-binary and membership-equality unit tests. Complete. |
| Alpha.8 → alpha.9 | Recording `Tum Hi Ho Bandhu` as direct intake exposed the pre-existing Celluloid order delta only in the next plan, after the wizard's one reorder scan. | Record-only deltas must converge cumulatively to a bounded fixed point after every proposal revision. Rebuild stale plans from the newest complete pull. | Intake → newly exposed reorder → empty plan fake-binary regression. Complete. |
| Alpha.12 → alpha.13 | Celluloid Mehfil and Kaveri Resonance alternated through repeated “Accepting current Spotify order” revisions and never stabilized. | Replaying historical assignment intent must be idempotent when the accepted provider membership already satisfies it; revision chronology must never become playlist order. | PostgreSQL regression preserves the exact provider order while extending an approved proposal; shell regression reaches one accepted baseline. Complete. |
| Alpha.12 → alpha.13 | Tracks added and later removed returned because record-only convergence had not created the immutable managed verification used to interpret the later removal. | Every exactly converged ordinary pull becomes the next accepted baseline. A later removal becomes an active exclusion and an older proposal cannot produce an add/restore operation. | PostgreSQL baseline/removal regression plus exclusion-archive lifecycle proof. Complete. |
| Alpha.15 → alpha.16 | A newly liked track already present in a managed playlist was silently summarized only as “Remove from Likes,” without naming its destination or remembering whether the user wanted both memberships. | Liked Songs is a virtual intake surface. Name every verified destination, require and revision a per-track keep/clear decision, default to no cleanup when undecided, and treat a later direct Unlike as superseding an older keep directive. | Fake-binary human-review regression plus disposable-PostgreSQL keep, clear, undecided, and direct-Unlike proof. Complete. |
| Alpha.17 → alpha.18 | Five Rasa Archive → Cinema Monsoon moves appeared twice, duplicate IDs stopped assignment, and the interrupted editable proposal made a retry label 1,439 existing tracks as direct intake. A later copy also replayed two excluded tracks into managed destinations. | One provider gesture yields one canonical move; an editable copy does not erase accepted coverage; active exclusions always outrank historical assignment revisions; interrupted work resumes cumulatively without expanding scope. | Exact paired-row fake-binary regression, classifier unit proof, disposable-PostgreSQL copy/exclusion proof, and live Neon-only recovery to zero pending operations. Complete. |
| Beta.1 candidate | Disconnect returned HTTP 403, and an out-of-band Spotify Apps revocation remained displayed as connected after observation failed. | History-preserving disconnect is a session-authenticated same-origin wrapper action. A terminal refresh-token rejection revokes the stale local envelope during the failed read, returns `authentication_required`, and renders Reconnect without deleting history. Connection presentation distinguishes locally authorized from last provider verification. | Same-origin/non-simple wrapper-header regression and terminal-versus-transient OAuth rejection regression. Complete in branch; deployment acceptance pending. |
| Beta.1 candidate | Spotify consent succeeded after Disconnect, but the callback left the connection disabled. | Vault generation is monotonic across active and revoked history and serialized by stable provider account; reconnect never reuses generation 1 or deletes audit history. | In-memory disconnect→reconnect regression and disposable-PostgreSQL provider lifecycle regression. Complete in branch; deployment acceptance pending. |
| Beta.1 candidate | Spotify observation captured a newly added eighth track in a managed playlist, while hosted maintenance reported `in sync` with zero changes and left the canonical model at seven. | Every direct addition to a managed provider surface must pass from the shared intake audit into the Rust maintenance DTO. One unambiguous destination is recorded automatically; simultaneous placement in multiple managed destinations remains one explicit decision. Liked state remains a separate choice. | Rust interpretation regressions cover unambiguous, ambiguous, and direct-add-plus-Like cases. A six-track fake provider account covers isolated gestures, composite snapshots, delayed observation, and interrupted retry on every CI run. Deployed; authenticated browser acceptance pending. |
| Beta.1 candidate | **Record these decisions** appeared inert when a destination plus Liked-state choice were submitted together. | Rust must issue each destination's typed opaque surface identity; the thin browser returns the selected DTO unchanged, submits every unresolved decision against one exact revision, and displays any rejected request without losing the user's ability to retry. | A Node browser-DTO harness covers destination, malformed destination, keep-Liked, remove-from-Likes, missing-choice, and missing-source cases; Rust locks the stable server-issued identity. Deployed; authenticated browser acceptance pending. |
| Beta.1 candidate | A composite review for a Liked-only track removed it from Liked Songs before its selected Neon Affection placement existed, then verification failed. The track was left in neither Spotify surface. | Consuming intake is never the first provider effect for a newly selected placement. Publish the exact enumerated destination addition, observe and verify it, then create a separate exact review for removing the saved state. Failure or restart at either stage must retain at least one provider copy, and replay must not duplicate membership. | The permanent stateful harness now uses the production maintenance DTO/state machine with a fake durable database and fake provider. It injects failure before both stages, reloads an Applying session after worker restart, replays an already accepted add, proves exact add-before-unlike order, and proves no loss or duplicate. Production recovery also interprets a pending ordinary `publish/add_track` as one exact addition. Full CI and exact-image deployment pass; authenticated recovery review remains and no automatic Spotify write is authorized. |
| Beta.1 candidate | Spotify exposed a Cinema Monsoon removal one pull before the matching Dakshina Pulse addition. Chordrift correctly accepted the first observed removal, but after the destination appeared it planned `managed_provider_drift` removal from Dakshina and the hosted interpreter failed with `state_conflict`. The first repair preserved Dakshina but labeled the event `direct intake` because the active exclusion had temporarily removed the canonical assignment. | Every new pull is cumulative provider truth. Any later single managed placement—back in the same playlist or in a new one—supersedes the active exclusion, with no timing window. Restore the exclusion in Neon, retain the exclusion's prior surface as provenance, and record restoration/reclassification without a provider write; multiple current destinations remain an explicit ambiguity. | Planner annotation tests cover one and multiple delayed destinations plus retained prior-source provenance. The fake-provider acceptance matrix covers later same-playlist restoration and new-playlist reclassification with zero provider writes. Live candidate proof preserved Dakshina with zero provider effects; beta.1 was held again until the label/provenance repair passed. |
| Beta.1 → beta.2 | Chrome warned on the private-beta CLI approval page and submitted the authenticated form without `Origin`; the strict same-origin guard returned HTTP 403. The installed CLI also buffered its fallback URL while waiting. | Browser approval accepts either an exact `Origin` or an exact parsed same-origin `Referer`; missing, scheme-mismatched, lookalike, and cross-origin provenance still fail closed. Flush the authorization URL before opening or waiting so another trusted browser can be used. PKCE, the authenticated subject, one-time flow, expiry, and loopback callback remain mandatory. | Unit regressions cover exact Referer acceptance and malicious/missing rejection. The normal CLI smoke must prove browser consent, secure credential storage, negotiation, and revocation. Chrome Safe Browsing reputation is an independent deployment issue and must not be bypassed by weakening application authorization. |
| Beta.2 → beta.3 | The server-side CLI consent repair loaded correctly, but interactive CLI presentation buffered its authorization URL until the callback completed, while the default Chrome profile continued to block the new private-beta hostname as suspected phishing. | Interactive login is a streaming operation. Print and flush the one-time URL before waiting, and support `--no-open` so the user can choose a trusted browser. A client fallback must never suppress, bypass, or weaken a browser reputation warning. | Parser and dispatch regressions prove login selects streaming output and accepts the explicit no-open flag. Release smoke uses the no-open path, authenticated consent, loopback PKCE callback, OS credential storage, and compatibility negotiation. |
| Beta.3 → beta.4 | The trusted in-app browser loaded the consent page but stripped both `Origin` and `Referer` from its form POST, so the beta.2 header fallback still returned 403. | CLI approval uses its own synchronizer token: generate a high-entropy one-time consent value on the authenticated GET, retain only its SHA-256 digest with the subject- and PKCE-bound flow, and require the exact value on POST. Headerless legitimate submission is accepted without weakening the general browser-mutation origin guard. Invalid, cross-subject, expired, approved, and replayed submissions fail closed and do not consume a valid attempt. | Unit regression proves valid headerless consent plus forged-token, wrong-subject, expiry, and replay rejection. Browser acceptance must complete the no-open CLI flow through the in-app browser, OS credential store, compatibility negotiation, and independent revocation. |

## Batched experience-refinement queue

These are usability improvements discovered during daily-driver testing that do
not invalidate the completed provider-first safety contract. Keep them together
until there is enough evidence to design and test one coherent interaction
checkpoint.

- **Rediscovered-favorite context.** When a newly liked track already exists in
  a managed playlist, show its destination, current one-based occurrence
  position, and playlist length. Treat the Like as a rediscovery signal rather
  than describing it only as redundant intake.
- **Optional resurfacing.** Offer `keep current position` or `move to top` after
  identifying a rediscovered favorite. The default is no reorder. Moving to the
  top must be a separately reviewed, snapshot-bound provider operation and must
  not be implied by the keep/clear Liked Songs answer.
- **New placement position.** A Liked-only track being placed into a destination
  for the first time defaults to the top and says so in the exact review. This
  is not the optional resurfacing case above: an already-present destination is
  never reordered implicitly. Future choices may add bottom or an exact
  position through the shared Rust contract.
- **Duplicate occurrences.** If the destination contains the same track more
  than once, show every occurrence or ask which occurrence should move; never
  silently collapse duplicates into one position.
- **Presentation sorting boundary.** Report canonical provider positions only.
  A temporary Spotify client sort by title, artist, album, or date added is not
  playlist order intent and is not currently part of the provider contract.
- **Provider/model membership explanation.** When a playlist count differs
  between the newest provider observation and Chordrift's model, show both
  directional sets—not only the net count. Name every provider-only and
  model-only track with its position and state timestamps. The same typed
  comparison must be renderable by the web client and remote CLI. A difference
  is pending observed intent, not automatically an error and never authority
  to overwrite either side.

## How to add the next finding

For every new failure, record:

1. the provider gesture and newest complete snapshot shape;
2. what the user expected;
3. what Chordrift inferred, recorded, planned, or attempted;
4. whether any Neon or provider write occurred;
5. the durable product rule, including authority and confirmation boundaries;
6. the smallest fake-provider/fake-binary regression that reproduces it; and
7. the checkpoint that ships the repair.

The always-on test fixture is `tests/provider_behavior_acceptance.rs`. It uses
the production maintenance DTOs and Rust state machine, a small stateful fake
provider, and a fake durable database that persists session revisions,
canonical placement intent, and exact write receipts across simulated worker
restarts. Keep its catalog deliberately small and synthetic. Add both
one-gesture and composite snapshots plus failure/retry cases when a new incident
is found; production account data and provider credentials never belong in this
harness.

Never normalize a failure as “operator error” merely because the user edited
Spotify between runs. Chaotic, cumulative provider use is the normal product
environment.
