# Daily-driver edge-case ledger

This ledger turns failures found through real Spotify use into durable product
rules and regression obligations. Read it with
`PLATFORM_INTENT_MODEL.md`. It contains no credentials or private library
inventory.

Status: active through the `v0.2.1-alpha.16` checkpoint, updated 2026-08-30.

## Governing rule

A complete Spotify pull is the newest baseline for user-authority state.
Chordrift records exact user gestures cumulatively and does not write them back,
reverse them, or require duplicate authorization. Ambiguous broader meaning may
remain unresolved. A Chordrift-authored change is separate, must be replanned
against the newest snapshot, and requires explicit authorization before any
provider write.

## Incidents and permanent regressions

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

## How to add the next finding

For every new failure, record:

1. the provider gesture and newest complete snapshot shape;
2. what the user expected;
3. what Chordrift inferred, recorded, planned, or attempted;
4. whether any Neon or provider write occurred;
5. the durable product rule, including authority and confirmation boundaries;
6. the smallest fake-provider/fake-binary regression that reproduces it; and
7. the checkpoint that ships the repair.

Never normalize a failure as “operator error” merely because the user edited
Spotify between runs. Chaotic, cumulative provider use is the normal product
environment.
