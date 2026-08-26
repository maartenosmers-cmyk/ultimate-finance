# Architecture

## System overview

```
 ┌────────────┐  ┌────────────┐  ┌──────────────┐
 │ iOS (Swift)│  │Android(Kt) │  │ Web/Desktop  │
 │  SwiftUI   │  │  Compose   │  │ (TS shell +  │
 │            │  │            │  │ Tauri option)│
 └─────┬──────┘  └─────┬──────┘  └──────┬───────┘
       │   REST + WebSocket (typed via OpenAPI codegen)
       └───────────────┼────────────────┘
                       ▼
             ┌───────────────────┐        ┌────────────────────┐
             │   API service     │◀──────▶│  planning-engine    │
             │  Rust · Axum      │ crate  │ pure-Rust math lib  │
             │  auth, CRUD,      │        │ amortize · Monte    │
             │  webhooks, WS     │        │ Carlo · solvers     │
             └───────┬───────────┘        └────────────────────┘
                     │ SQLx (pooled)
                     ▼
             ┌───────────────────┐     ┌──────────────┐
             │  PostgreSQL       │     │ Redis        │
             │  ledger + ts data │     │ cache/queues │
             └───────────────────┘     └──────┬───────┘
                                              ▼
                                     ┌────────────────────┐
                                     │  Sync workers      │
                                     │  Plaid/Teller/MX   │
                                     │  adapters behind   │
                                     │  Aggregator trait  │
                                     └────────────────────┘
```

## Key decisions

### 1. Bank aggregation: one server-side abstraction
`Aggregator` trait (`connect`, `refresh`, `handle_webhook`, `map_transactions`).
Plaid implemented first; Teller/MX later. Webhooks are **idempotent** (event-id dedup table).
Clients embed the provider's Link SDK only for credential capture — everything else is ours.

### 2. Rust everywhere it matters
- Money correctness: `i64` minor units in transport/storage; `rust_decimal` where decimals are unavoidable.
- Latency under load: no GC pauses; async tokio runtime scales to many concurrent users.
- One math core: `planning-engine` is a plain crate → same code runs on the server,
  compiled to WASM for web, and via UniFFI into Swift/Kotlin. Planning feels *instant* on-device.

### 3. Data model spine
Households own accounts; accounts own transactions and balance snapshots.
Time-series tables (`balance_snapshots`, `scenario_runs`) use `(owner_id, at)` composite keys,
partition-friendly. See `db/migrations/0001_init.sql`.

### 4. Scenarios are first-class rows
A scenario snapshots normalized financial state as JSONB + references live taxonomies.
Runs persist inputs/outputs so results are reproducible and shareable with partners/advisors.

### 5. Performance contract
- Reads: indexed queries only, cursor pagination, Redis cache for dashboards (short TTL, bust on write).
- Writes: transactional outbox for downstream effects.
- Sync: per-connection job queue with exponential backoff; progress pushed over WebSocket.

## Client strategy

| Platform | UI | Notes |
|---|---|---|
| iOS/iPadOS/macOS | SwiftUI | Shared Swift package wraps UniFFI planning bindings |
| Android | Jetpack Compose | Material 3 dynamic theming |
| Web/desktop | TypeScript SPA | Same design system; Tauri shell optional |

Design system defined once (tokens: color/type/spacing/motion) and ported per platform —
native feel is non-negotiable; no Electron-style lowest-common-denominator UI.
