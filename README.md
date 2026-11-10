# Stellar Stream Vault

Real-time programmable payment streams on Soroban — built for the **Drips Wave x Stellar** program.

Senders lock any Stellar Asset Contract (SAC) token (XLM, USDC, ...) into a stream; recipients withdraw vested funds linearly over time, gated by an optional cliff. Streams can be cancellable (vested → recipient, unvested → sender refund) or locked.

- Contract: `contracts/stream_vault` (Soroban / Rust)
- Frontend: `frontend/` (Next.js + `@stellar/stellar-sdk` + `@stellar/freighter-api`)
- Architecture + security invariants: [ARCHITECTURE.md](./ARCHITECTURE.md)

## Prerequisites

- Rust stable + `wasm32-unknown-unknown` target (`rustup target add wasm32-unknown-unknown`)
- [Stellar CLI](https://developers.stellar.org/docs/tools/cli/install-cli): `cargo install --locked stellar-cli --features opt`
- Node.js 18+
- [Freighter wallet](https://www.freighter.app/) (browser extension) for Testnet

## Build & test the contract

```bash
cd contracts/stream_vault
cargo test
stellar contract build
```

The optimized WASM lands in `target/wasm32-unknown-unknown/release/stream_vault.wasm`.

> The committed `Cargo.lock` pins `ed25519-dalek` to v2, which `soroban-sdk`
> v22 requires (v3 breaks its build). Keep the lockfile when building.

## Deploy to Stellar Testnet

```bash
# 1. Configure an identity (funded via friendbot on testnet)
stellar keys generate deployer
stellar keys fund deployer

# 2. Deploy
stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/stream_vault.wasm \
  --source deployer \
  --network testnet

# 3. Initialize (one time) — save the returned contract id as C...
stellar contract invoke \
  --contract-id <CONTRACT_ID> \
  --source deployer \
  --network testnet \
  -- initialize --admin $(stellar keys address deployer)
```

Mainnet uses `--network mainnet` with a funded identity. See [ARCHITECTURE.md](./ARCHITECTURE.md) for state layout and invariants.

## Quick CLI flows (testnet)

```bash
SENDER=$(stellar keys address deployer)
RECIPIENT=<G... recipient>
TOKEN=<C... SAC token, e.g. native XLM wrapper>
NOW=$(date +%s); START=$((NOW+120)); END=$((START+2592000))

# Create a 30-day cancellable stream
stellar contract invoke --contract-id <CONTRACT_ID> --source deployer --network testnet \
  -- create_stream --sender $SENDER --recipient $RECIPIENT --token $TOKEN \
  --amount 10000000 --start $START --cliff $START --end $END --cancellable true

# Views
stellar contract invoke --contract-id <CONTRACT_ID> --source deployer --network testnet \
  -- get_stream --stream_id 0
stellar contract invoke --contract-id <CONTRACT_ID> --source deployer --network testnet \
  -- withdrawable --stream_id 0
```

## Run the frontend

```bash
cd frontend
cp .env.example .env.local
# set NEXT_PUBLIC_VAULT_CONTRACT_ID=<C...> in .env.local
npm install
npm run dev
```

Open `http://localhost:3000`, connect Freighter (Testnet mode), then create / load / withdraw / cancel streams. All invocations are simulated before signing; failures surface with the simulation error.

## Contract API

| Method | Auth | Description |
|---|---|---|
| `initialize(admin)` | admin | One-time setup |
| `create_stream(sender, recipient, token, amount, start, cliff, end, cancellable)` | sender | Fund + open stream, returns id |
| `withdraw(stream_id)` | recipient | Pull vested funds, returns amount |
| `cancel(stream_id)` | sender | Settle vested → recipient, rest → refund |
| `get_stream(stream_id)` | — | Full stream struct |
| `vested(stream_id)` | — | Vested amount at current ledger time |
| `withdrawable(stream_id)` | — | `vested - withdrawn` (0 if cancelled) |
| `list_streams(offset, limit)` | — | Paginated ids (max 50) |

Amounts are `i128` in stroops (7 decimals). Times are unix seconds; `start < end`, `start <= cliff <= end`, `start >= now` enforced.

## Wave submission notes

- One-command verification: `cargo test` passes with 4 tests (vesting math, init replay, time validation, full SAC lifecycle).
- No admin backdoor: admin exists only to mark initialization; funds move only via stream rules + `require_auth`.
- Events emitted: `init`, `created`, `withdraw`, `cancel` for indexer support.

## Verified toolchain

| Check | Result |
|---|---|
| `cargo test` (contract) | 4/4 pass, zero warnings |
| `cargo clippy --all-targets` | clean |
| `cargo fmt --check` | clean |
| WASM release build (`stream_vault.wasm`, 36 KB) | OK |
| `tsc --noEmit` + `next build` (frontend) | OK |

Pinned versions: `soroban-sdk` 22.0.11, `@stellar/stellar-sdk` 13.3.0,
`@stellar/freighter-api` 4.1.0, `next` 14.2.5, Rust 1.96.
