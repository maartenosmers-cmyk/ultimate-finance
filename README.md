# Ultimate Finance App

A personal-finance platform that tracks, budgets, plans — and goes beyond Monarch with a
true **scenario & planning engine** (what-if simulations, retirement/FIRE modeling,
mortgage comparison, tax-lot-aware projections).

> Working name; trivially renamable (folder + `workspace.package.name`).

## Layout

| Path | What it is |
|---|---|
| `docs/` | Feature matrix vs Monarch, architecture, roadmap |
| `crates/planning-engine/` | Pure-Rust financial math (amortization, Monte Carlo, goal forecasting). Dependency-light so it can later compile to WASM + UniFFI for on-device instant sims |
| `services/api/` | Main API server (Rust / Axum): auth, ledger, **bank aggregation** |
| `db/migrations/` | Postgres schema, forward-only migrations |
| `apps/ios/` | **Native SwiftUI client** (iOS 17+, zero deps) — see `apps/ios/BUILD.md` |
| `apps/android/` | Compose client — planned |

## Bank connectivity

Aggregation runs server-side behind one provider abstraction (`src/aggregate/`):

- **Plaid** — full client implemented (`aggregate/plaid.rs`). Set
  `PLAID_CLIENT_ID`, `PLAID_SECRET`, `PLAID_ENV=sandbox` and Link flows go live.
- **Mock** — deterministic fake institution (`POST /api/v1/connections/mock-connect`)
  so the whole connect → sync → dedup → webhook pipeline works with zero credentials.

Synced accounts are *provider-authoritative*: balances come from the institution,
transactions are deduped by external id, and webhooks are idempotent by body hash.

## Quickstart (backend)

```powershell
cd services/api
cargo run          # serves http://localhost:8080
```

Try it:

```powershell
$s = irm -Method Post localhost:8080/api/v1/auth/signup -ContentType application/json `
  -Body '{"email":"you@x.io","password":"password123","displayName":"You"}'
$h = @{ Authorization = "Bearer $($s.token)" }
irm -Method Post localhost:8080/api/v1/accounts -Headers $h -ContentType application/json `
  -Body ('{"householdId":"' + $s.household.id + '","type":"checking","name":"Checking","currentBalanceMinor":500000}')
```

Run the test suites (30 tests):

```powershell
cargo test         # from repo root: planning-engine + API integration tests
```

## Product pillars

1. **Everything Monarch does** — aggregation across 13k+ institutions, unified transactions,
   auto-categorization, recurring/subscription detection, flexible budgets, goals, cash-flow,
   customizable reports, partner/advisor collaboration.
2. **Planning as the killer feature** — not charts-of-the-past but *models of the future*:
   side-by-side what-if scenarios ("what if we move in June and I switch to contracting?"),
   Monte Carlo success probability, mortgage refi comparisons, tax-aware withdrawal ordering.
3. **Speed as a feature** — sub-100ms p50 API reads, optimistic UI everywhere, background sync
   workers; money math runs locally on-device where possible (WASM/native) so planning feels instant.

## Status

Phase M0 (foundation) — see `docs/ROADMAP.md`.
