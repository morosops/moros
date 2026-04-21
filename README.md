# Moros

Moros is a Starknet casino protocol. This repository currently tracks the on-chain contract layer and the blackjack dealer-peek circuit source.

## Contracts

The Cairo workspace is rooted at `Scarb.toml` and the contract package is in `contracts/`.

Core contracts:

- `BankrollVault`: STRK custody, player wallet/vault balances, house liquidity, exposure reservation, player withdrawals, and timelocked house withdrawals.
- `RewardsTreasury`: isolated rewards funding with global and per-operator budget limits.
- `SessionRegistry`: session-key authorization limits for gameplay actions.
- `TableRegistry`: table ids, table addresses, and table wager bounds.
- `DealerCommitment`: server-seed commitment registry used by dice, roulette, and baccarat.
- `DeckCommitment`: Poseidon Merkle-root commitments and reveal tracking for blackjack decks.
- `BlackjackTable`, `DiceTable`, `RouletteTable`, `BaccaratTable`: game-specific state machines, wager caps, timeout recovery, and payout settlement.
- `Groth16VerifierBN254`: Garaga-generated verifier used by blackjack dealer-peek validation.

The committed tests under `contracts/tests/` cover balance segregation, reward treasury limits, game caps, timeout paths, session authorization, blackjack commitment binding, and roulette/dice/baccarat settlement constraints.

## Circuit

`circuits/blackjack/dealer_peek_no_blackjack.circom` is the blackjack dealer-peek circuit. It proves that the dealer hole card opening is bound to the committed deck root and that the public peek result matches the blackjack predicate for Ace or ten-value upcards.

Generated circuit artifacts are intentionally not committed. Build outputs belong under `circuits/build/`, which is ignored.

## Verification

Run contract tests:

```bash
cd contracts
snforge test
```

Expected local result:

```text
74 passed, 0 failed, 2 ignored
```

The ignored tests are live fork checks for the real Groth16 verifier fixture and require an external Starknet RPC. They are not part of the default deterministic local test run.
