# Stellar Stream Vault — Wave Program Architecture

**Selected concept:** Real-time programmable payment streams on Soroban.

Why this pick for Drips Wave x Stellar (Jan 2026 launch partner): Drips is a streaming-payments protocol. A native Stellar streaming primitive is the highest-alignment, highest-impact build — it brings Drips-style "Fix, Merge, Earn, Stream" to Soroban, and fits Stellar's payments + USDC + anchors thesis.

## System overview

```
Sender (Freighter) ──deposit SAC──> StreamVault (Soroban)
                                      │ linear vesting + cliff
                                      │ cancellable / non-cancellable
                                      └─withdraw──> Recipient (Freighter)
```

Components:

1. `contracts/stream_vault` — Soroban contract, holds SAC balances, enforces vesting math.
2. `frontend/` — Next.js + `@stellar/stellar-sdk` + `@stellar/freighter-api`, builds/simulates/submits Soroban txs.
3. Stellar Asset Contract (SAC) — any Stellar token (XLM-wrapped, USDC) via standard token interface.

## Contract state

```rust
enum DataKey { Admin, Counter, Stream(u64) }

struct Stream {
  sender: Address, recipient: Address, token: Address,
  deposit: i128, withdrawn: i128,
  start: u64, cliff: u64, end: u64,
  cancellable: bool, cancelled: bool, created_at: u64,
}
```

- `Instance`: `Admin`
- `Persistent`: `Counter: u64`, `Stream(id): Stream`
- TTL extended on every read/write (instance + persistent).

## Core functions

- `initialize(admin)` — one-time admin set.
- `create_stream(sender, recipient, token, amount, start, cliff, end, cancellable) -> u64` — pulls SAC via `transfer`, stores stream.
- `withdraw(stream_id)` — recipient pulls `vested - withdrawn`.
- `cancel(stream_id)` — sender cancels; vested goes to recipient, remainder refunded to sender.
- `get_stream(stream_id)`, `vested(stream_id)`, `withdrawable(stream_id)`, `list_streams(offset, limit)`, `admin()`, `stream_count()` — views.

Vesting math (linear, cliff-gated):

- `t < cliff` → 0
- `t >= end` → deposit
- else → `deposit * (t - start) / (end - start)`

## Security invariants

1. Auth: `sender.require_auth()` on create/cancel; `recipient.require_auth()` on withdraw. Admin only for `initialize`.
2. No over-withdraw: `withdrawn <= vested <= deposit` always; checked math, no division by zero (`end > start` enforced).
3. Custody: contract never holds more than sum of un-withdrawn deposits; cancel settles both legs atomically.
4. Cancellation: non-cancellable streams revert on cancel; `withdraw` on a cancelled stream reverts and `withdrawable` returns 0.
5. Time: `start < end`, `start <= cliff <= end`, `start >= ledger.timestamp` at creation.
6. Token safety: generic SAC via `TokenClient`; amount `> 0`; zero-address rejected by SDK Address type.
7. Storage: monotonic `Counter`, no ID reuse; TTL extension prevents archival mid-stream.
