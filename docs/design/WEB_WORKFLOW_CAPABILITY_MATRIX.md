# Daily-driver client capability matrix

Status: active post-V021-06 private-beta contract at `v0.2.1-beta.10`. The
browser and installed CLI are thin clients of the same typed Rust application
contract. This document does not authorize a provider write or a generic
command, shell, SQL, or provider-token endpoint.

## Provider context is always visible

Every account screen that presents music state must show the selected provider
connection. The selector is an account-owned connection, not a global product
setting, and must support more than one connection without changing the
workflow DTOs.

For the selected connection, the client shows:

- provider name and account display name;
- `Connected`, `Authorization required`, or `Revoked` credential state;
- the time of the newest complete provider observation;
- whether a view represents that provider observation or Chordrift's current
  model; and
- a visible read-only/writes-disabled state while the deployment gate is
  closed.

`Provider observation` means the newest complete state already recorded by
Chordrift. It must not be labeled live unless a pull in the current operation
has completed. `Chordrift model` means durable collection intent and may differ
from the provider observation while maintenance is pending.

## Private-beta workflow matrix

Unless a row explicitly says otherwise, both web and CLI must render and drive
the workflow. The web client uses controls; the CLI uses the interactive
`service maintenance wizard`. Both submit the same DTOs and observe the same
revision, authorization, and verification gates. Raw CLI JSON commands are
diagnostic/automation primitives, not an alternate maintenance implementation.

### Wrapper parity

| Capability | Web | Remote CLI | Shared authority |
| --- | --- | --- | --- |
| Chordrift identity | Login/logout controls | `service session login/remove` | Auth0 identity exchange and account-scoped Chordrift sessions |
| Provider identity | Connect, reconnect, disconnect, select | Connect, reconnect, disconnect, list; explicit connection ID selects context | Account-owned provider connections and encrypted credential vault |
| Ordinary maintenance | Guided controls | `service maintenance wizard` | Durable operation and maintenance-session DTOs |
| Resume/recovery | Reload active session and operation | `wizard --session-id`; operation/session IDs printed immediately | PostgreSQL session revisions, operation events, idempotency and cancellation |
| Library investigation | Library, comparison, track and Excluded views | Typed query commands; selected expert JSON diagnostics | Provider-scoped immutable query views |
| Exact provider writes | Default-no confirmation of displayed effects | Default-no prompt for the identical displayed effects | Server-rederived review ID, revision check, apply/observe/verify worker |
| Operator automation | Not exposed | Raw command/query files, safe diagnostics, release checks | Same authenticated contract; never shell, SQL, credentials, or provider URLs |

Friendly CLI library renderers may continue to improve independently, but all
launch queries and provider-lifecycle operations have explicit remote CLI
commands. The underlying typed operations are shared; no music behavior waits
on a wrapper.

| Product surface | Rust authority | Browser responsibility | Provider effect | V021-06 state |
| --- | --- | --- | --- | --- |
| Product login/logout | OIDC exchange, Chordrift session, account authorization and revocation | Start login, show signed-in state, request logout | None | Deployed |
| Provider status/selection | List tenant-owned provider connections, credential state and observation time | Render selector and selected context everywhere | None | Deployed for Spotify |
| Provider connect/reconnect/disconnect | Match stable provider identity, provision or rotate an encrypted credential, or revoke it without deleting retained account data | Launch provider OAuth and distinguish Disconnect from Chordrift logout | Authorization only; no library mutation | Deployed; existing retained connection correctly awaits one explicit Reconnect OAuth |
| Multiple provider accounts | Isolate every observation, intent record, credential and operation by account-owned provider connection | Add and switch explicit connections without merging their state | None | PostgreSQL tenant proof passed; browser acceptance pending |
| Provider playlists | Query newest complete provider observation | List playlists and counts with observation time | None | Deployed |
| Provider playlist tracks | Query exact ordered observed membership | Render one-based custom order, title, artists and album | None | Deployed |
| Chordrift model playlists | Query newest current model generation | Render separately from provider state | None | Deployed |
| Chordrift model tracks | Query exact ordered model membership | Render separately from provider state | None | Deployed |
| Provider/model comparison | Return directional membership, unresolved-identity and custom-order differences with both state timestamps | Explain provider-only and model-only counts instead of only unequal totals | None | Deployed and browser-proven; Lightleak Reverie explains 501/501 as order-only drift |
| Track detail | Join identity, placements, saved state, exclusions and personal listening statistics | Render facts without inferring classification | None | Deployed |
| Exclusion archive | Query active reversible exclusions and prior placement | Search/filter, inspect and begin an explicit restore or forget flow | None until separately confirmed | Read-only deployed; actions pending |
| Observe changes | Lease provider credential, verify stable identity, pull one complete snapshot, refresh derived state and persist durable progress | Start/reconnect, display progress and allow cancellation | Provider reads only | Deployed and release-smoked |
| Ordinary maintenance | Interpret cumulative provider gestures, project resolved record-only intent, return only genuine ambiguity and exact effects | Start, follow, refresh, and render one shared session; never assemble internal plans | None until exact authorization | Deployed through Web and remote CLI |
| Saved/Liked intake | Detect new, rediscovered and already-placed tracks; remember keep/consume decisions; clear only after destination intent | Ask only when policy is unresolved; render the exact cleanup review | Optional exact saved-state change | Deployed with separate placement and cleanup reviews |
| Direct playlist intake | Treat a provider-side add as placement evidence and canonical intake | Explain the accepted placement or request genuine ambiguity | Usually none | Deployed with regression coverage |
| Reclassification | Interpret a paired managed remove/add as one move and training signal | Show the inferred move; request a destination only if ambiguous | Usually none | Deployed with delayed-observation reconciliation coverage |
| Reorder | Accept exact provider custom order when membership is equal | Show it as recorded intent, not a provider mutation | None | Deployed |
| Removal/exclusion | Record a managed removal as a reversible exclusion | Explain archive semantics and offer later restore/forget | None for the observed removal | Record path deployed; restore/forget browser actions pending |
| Exact provider review | Bind human labels and effects to one immutable review/revision; rederive trusted effects before execution | Render exact effects and submit only revision plus review identity | Enumerated, reviewed effect only | Deployed and exact-write release-smoked |
| Operations/activity | Persist lifecycle, progress, events, retry, cancellation and receipts | Reconnect, page events, cancel/retry only when allowed | Depends on operation | API/worker deployment and restart persistence proven |
| Recovery candidates | Rank history-known tracks absent from playlists/saved state while preserving exclusions | Filter, inspect and choose explicit restoration | Only after a later exact review | Post-deployment audit |
| Spin | Build deterministic preview and separately authorize publication | Dedicated workflow, never ordinary maintenance | Separately reviewed publication | Later web surface |
| Playlist creation, artwork and retirement | Dedicated typed workflows and safety gates | Dedicated review surfaces | Separately reviewed mutation | Later web surfaces |
| Diagnostics | Return safe capability, schema, deployment and operation facts | Copy structured diagnostics; never receive secrets | None | Compatibility and developer lab deployed |

## Web acceptance rules

1. A signed-out page shows Google login; a signed-in page shows account and
   logout state without also showing a login action.
2. Every music query is tenant-scoped and provider-connection-scoped. An opaque
   resource ID never grants access by itself.
3. Refreshing or retrying a browser request is idempotent. It cannot repeat a
   provider effect or lose an unfinished operation.
4. A provider-side edit is accepted as the newest observed state by default.
   Chordrift asks about broader meaning only when it cannot safely infer it.
5. Record-only gestures converge without provider authorization. Chordrift-
   authored writes require one exact, human-readable review tied to the newest
   provider snapshot.
6. Provider writes are limited to exact, server-rederived maintenance effects.
   Placement and intake cleanup are separate durable reviews: add, observe and
   verify first; only then may a later review remove temporary membership.
7. Browser tests cover login visibility, provider context, both library state
   planes, track details, exclusions, session expiry and the critical
   maintenance journey. Transport/fake-provider tests remain the primary proof
   of domain behavior, idempotency and tenant isolation.
