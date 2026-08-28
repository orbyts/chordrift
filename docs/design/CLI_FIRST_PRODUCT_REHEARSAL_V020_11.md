# CLI-first product rehearsal — V020-11

Status: implemented on the v0.2.0 development line. These commands are not in
the released v0.1.4 binary and are not a replacement for its daily workflow.

## Outcome

V020-11 exposes the provider-neutral work from V020-06 through V020-10 through
one consistent `chordrift product` namespace:

- `product onboarding capture` supplies a validated JSON fixture to the
  mutation-free onboarding provider port and persists either the inventory-only
  or explicitly enriched session;
- `product onboarding audit` reads the corresponding Rust-owned audit view;
- `product collections list` reads account-owned collection boundaries and
  current membership counts;
- `product recipes show` reads one immutable account-owned recipe revision;
- `product recipes execute` sends validated fixture candidates through the
  V020-09 executor and displays the exact unordered draft;
- `product spins preview` sends that draft through the V020-10 orderer and
  persists the exact preview; and
- `product spins show` reloads the same account-owned preview.

Every command enters through `ApplicationFacade`. The CLI owns argument/file
parsing and a common stable output envelope only. It does not reproduce audit,
selection, capability, ordering, fingerprint, ownership, or replay behavior.

## Inputs and output

`OnboardingRehearsalFixture` contains one validated `AccountContext`, an
inventory-only `OnboardingInputs` value, and a separate enriched value with the
explicit extended-history evidence. The fixture reader implements only the
existing mutation-free `OnboardingProviderReader`; it has no write method.

`SpinRehearsalFixture` contains an explicit owner, one validated
`RecipeExecutionRequest`, its exact evidence capability snapshot, and a `u64`
seed. Recipe execution and Spin creation consume those Rust values directly.

Redirected output from every product leaf starts with:

```text
product_view: KIND
contract_version: 1.0
provider_writes: disabled
```

It then provides important identities/fingerprints as stable key/value lines
and the complete serialized Rust value on one `value_json` line. The helper
uses only the identity lines; it never parses presentation prose to recreate a
domain decision.

## Installed-binary workflow

`scripts/chordrift-product-rehearsal.sh` selects the installed development
binary through `CHORDRIFT_BIN`. It captures and audits both onboarding modes,
proves the enriched result retained the exact comparable inventory findings
(complete audit fingerprints correctly differ by session), reviews
collections and the persisted recipe, executes the recipe, creates a Spin, and
proves the reloaded preview fingerprint is unchanged.

The helper never invokes `cargo run`, `db migrate`, `spotify`, `sync apply`,
publication approval, or any provider command. A fake-binary shell test proves
the complete command sequence and these forbidden-command absences.

## Safety boundary

All database-backed product commands require
`CHORDRIFT_PRODUCT_REHEARSAL=1`. This is an intentional tripwire, not a database
selector: the operator must also point `CHORDRIFT_DATABASE_URL` at an isolated
database where migration 0046 was already applied. The commands never apply a
migration themselves. Recipe execution is database- and provider-free, but
Spin preview creation persists to the isolated migration-0046 tables.

V020-11 adds no provider adapter method, provider credential access,
publication approval, plan conversion, or provider write. It did not access
production Neon, call Spotify, or apply migration 0046.

## Next required gate

Before V020-12, main must reconcile the recovered v0.1.x 92-track incident.
That task will selectively port the enumerated playlist-write correction and
complete operator intake workflow, add the binary capability handshake and
fake-binary compatibility proofs, separate maintenance plans from future Spin
publication plans, and update V020-12 through V020-14 acceptance criteria. The
older maintenance branches must not be merged wholesale.

Compatibility in that gate preserves safety invariants and operator outcomes,
not every old command spelling or internal execution path. The provider-neutral
v0.2 Rust contract remains authoritative; compatibility adapters and the binary
handshake must fail visibly rather than degrade ownership, determinism,
plan-origin separation, or publication safety.
