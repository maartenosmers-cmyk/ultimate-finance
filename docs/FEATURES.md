# Feature matrix: Monarch vs. this project

Legend: ✅ shipping · 🚧 in progress · 📋 planned · 💎 exceeds Monarch

## Core tracking (parity target)

| Capability | Monarch | Ours | Notes |
|---|---|---|---|
| Bank/card/loan/investment aggregation | ✅ (13k+ FI, multi-provider) | 📋 Plaid first, Teller/MX adapters behind one `Aggregator` trait | Aggregation is server-side; every client benefits |
| Manual & offline accounts (house, car, art) | ✅ | ✅ M1 | |
| Unified transaction list, review workflow | ✅ | ✅ M2 | Split transactions + receipt OCR 💎 |
| Auto-categorization w/ user rules | ✅ | ✅ M2 | Rules engine + embeddings fallback; per-household learning |
| Recurring & subscription detection | ✅ | ✅ M2 | Also detects *amount drift* and price hikes 💎 |
| Net worth, cash flow, spending trends | ✅ | ✅ M2 | Balance snapshots as time-series |
| Custom dashboards | ✅ | ✅ M3 | Widget grid, saved layouts |
| Reports (custom, savable) | ✅ | ✅ M3 | |
| Budgets (flex/fixed/rolling) | ✅ | ✅ M3 | |
| Goals with progress + funding plans | ✅ | ✅ M3 | |
| Bill calendar | ✅ | ✅ M2 | |
| Partner collaboration (shared household) | ✅ free | ✅ M4 | Plus granular per-account visibility scopes 💎 |
| Advisor/professional read-only access | ✅ paid tier | ✅ M4 | |
| Web + iOS + Android sync | ✅ | ✅ | Native SwiftUI / Compose / web shell |

## Planning engine (💎 the differentiator)

| Capability | Notes |
|---|---|
| What-if scenario branching | Fork your real financial state into named scenarios; diff outcomes side-by-side ("stay vs. move", "W-2 vs. contracting") |
| Monte Carlo retirement/FIRE success probability | Portfolio returns sampled from historical/bootstrap distributions; percentile fan charts |
| Goal probability & required-savings solver | Solve backwards: "to retire at 52 at 85% success you need $X/mo" |
| Mortgage/refi lab | Amortization comparison: term/rate/extra-payment/points/PITI, breakeven dates |
| Tax-aware projections | Marginal-bracket modeling; tax-lot-aware withdrawal ordering (lots table exists from day one) |
| Income-event modeling | Raise, sabbatical, RSU vest schedules, Social Security timing |
| On-device instant sims | planning-engine compiles to WASM/native → drag sliders, results recompute <16ms, no round-trip |

## Quality bar (💎 across everything)

- p50 API read < 100ms; sync jobs stream progress to clients over WebSocket
- Every money value stored as integer minor units or `Decimal` — never floats in ledgers
- Full audit trail on mutations; idempotent webhook ingestion
- Local-first caches on mobile; app is usable offline for reads
