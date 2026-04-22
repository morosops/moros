# Moros

Moros is a on-chain casino stack with on-chain game contracts, a React client, and backend services for gameplay orchestration, deposits, indexing, auth bridging, and verifier/prover support.

## Repository Layout

- `contracts/`: Cairo contracts and tests.
- `circuits/`: blackjack dealer-peek Circom source.
- `services/`: Rust/Node backend services.
- `app/`: React + Vite frontend.

## On-Chain Contracts

The Cairo workspace is rooted at `Scarb.toml`.

Core contracts:

- `BankrollVault`: STRK vault ledger for player wallet balance, player vault balance, reserved funds, house liquidity, and timelocked house withdrawals.
- `RewardsTreasury`: separate rewards pool with global budget caps and per-operator spending limits.
- `SessionRegistry`: session-key authorization and action limits for gameplay.
- `TableRegistry`: canonical table ids, table contract addresses, wager caps, and table metadata.
- `DealerCommitment`: server-seed commitment registry for dice, roulette, and baccarat.
- `DeckCommitment`: Poseidon Merkle-root deck commitment and reveal bitmap tracking for blackjack.
- `BlackjackTable`: Vegas-strip blackjack state machine with Groth16 dealer-peek verification.
- `DiceTable`: commit / open / reveal / verify dice flow with bounded wagers.
- `RouletteTable`: European roulette settlement and bet validation.
- `BaccaratTable`: baccarat round settlement and wager validation.
- `Groth16VerifierBN254`: Garaga-generated verifier consumed by blackjack dealer-peek validation.

Supporting Cairo modules:

- `blackjack_logic.cairo`
- `interfaces.cairo`
- `types.cairo`

Contract tests under `contracts/tests/` cover vault segregation, reward limits, table caps, blackjack deck binding, timeout paths, and dice / roulette / baccarat settlement rules.

## Blackjack Circuit

`circuits/blackjack/dealer_peek_no_blackjack.circom` proves the dealer peek predicate for Ace / ten-value upcards against the committed deck leaf used by blackjack.

Generated proving artifacts are intentionally not committed. Build outputs belong under `circuits/build/`, which is ignored.

## Backend Services

Rust services:

- `services/game-coordinator`: main gameplay API, profile and funding coordination, rewards, and contract reads/writes.
- `services/relayer`: relayed gameplay action submission for active hand runtimes.
- `services/indexer`: on-chain event indexing into the read model.
- `services/deposit-router`: deposit address issuance, deposit tracking, and routing state.
- `services/common`: shared persistence, models, migrations, and contract client helpers.
- `services/blackjack-prover`: blackjack proving helpers.
- `services/blackjack-verifier`: verifier-side validation service support.

Node services:

- `services/privy-bridge`: Privy auth bridge, embedded Starknet wallet coordination, paymaster bridge, and Moros account resolution.
- `services/deposit-executor`: executes routed deposit jobs.

Database migrations live in `services/common/migrations/`.

## Frontend

`app/` is a React 19 + Vite application using:

- `@privy-io/react-auth` for Google / email / wallet login
- `starkzap` for Starknet wallet and paymaster integration
- `zustand` for client state
- `@tanstack/react-query` for server state

The frontend consumes the coordinator, relayer, auth bridge, and deposit-router APIs and renders the game flows for dice, blackjack, roulette, baccarat, wallet, rewards, leaderboard, and profile UX.

## Local Verification

Contracts:

```bash
cd contracts
snforge test
```

Services:

```bash
cargo check --workspace
```

Frontend:

```bash
npm --prefix app run test
npm --prefix app run build
```

