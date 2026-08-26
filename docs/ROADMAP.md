# Roadmap

## M0 — Foundation (current)
- [x] Monorepo layout, docs, decisions
- [x] Postgres schema v1 (households → accounts → transactions → budgets/goals/scenarios/holdings)
- [x] API service skeleton (health endpoint, config, tracing)
- [x] planning-engine: amortization, Monte Carlo retirement sim + tests

## M1 — Ledger core (API usable)
- [x] Auth (argon2 passwords, opaque sessions; passkeys next), households & auto-starter-household
- [x] Accounts CRUD w/ visibility scopes + liability sign conventions
- [x] Transaction CRUD with atomic balance invariants + cursor pagination
- [ ] Passkey/WebAuthn login; invites & partner joins (pulled from M4)
- [ ] Category tree + user rules engine
- [ ] CSV import (multi-bank format heuristics)
- [ ] Postgres via SQLx behind the store facade (memory impl used for dev/tests)

## M2 — Aggregation & automation
- [x] Provider abstraction (`Aggregator` enum: Plaid + deterministic mock)
- [x] Plaid client: link-token, token exchange, accounts/get, transactions/sync w/ cursors
- [x] Mock institution: instant dev connection (`mock-connect`) usable without keys
- [x] Idempotent webhook pipeline (body-hash dedup) + pull-on-webhook sync
- [x] External-id upserts; synced accounts are provider-authoritative for balances
- [ ] Plaid webhook JWT verification (currently sandbox-friendly)
- [ ] Recurring/subscription detection (+price-hike alerts)
- [ ] Bill calendar
- [ ] Auto-categorization: rules → model fallback

## M3 — Budgets, goals, dashboards, reports
- [ ] Flexible budgets (monthly/flexible/rolling categories)
- [ ] Goals with funding plans and progress math from planning-engine
- [ ] Dashboard widget grid (saved layouts)
- [ ] Reports builder (group-by, filter, trend, savable)

## M4 — Multiplayer & mobile shells
- [x] iOS SwiftUI app skeleton: auth, home/net-worth, account detail,
      manual transactions, mock bank connect, settings (see `apps/ios/BUILD.md`)
- [ ] Partner/advisor roles UI, per-account visibility scopes
- [ ] Plaid Link SDK integration in Connections screen
- [ ] Android Compose app: same scope
- [ ] WebSocket push: sync progress, anomaly/budget alerts

## M5 — Planning surfaces (the differentiator ships to users)
- [ ] Scenario editor UI (branch real state → tweak → diff)
- [ ] Monte Carlo fan charts; goal probability solver
- [ ] Mortgage/refi lab
- [ ] On-device WASM/native sims

## M6 — Polish & scale
- [ ] Offline-first caches, optimistic sync everywhere
- [ ] Tax-lot-aware withdrawal ordering in projections
- [ ] Load testing to the performance contract; partitioning balance history
- [ ] Billing (subscription), admin console
