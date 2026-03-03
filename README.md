# gonka-thesoul-vesting

CosmWasm smart contract for time-locked GNK token vesting on the Gonka chain.

## Overview

This contract holds GNK tokens and releases them to a fixed recipient address according to a predefined quarterly schedule. The Gonka community governance holds pause and emergency withdrawal controls.

### Vesting Schedule

| Tranche | GNK Amount | Unlock Time | ngonka (on-chain) |
|---------|-----------|-------------|-------------------|
| 0 | 500,000 GNK | Immediate (at instantiation) | 500,000,000,000,000 |
| 1 | 150,000 GNK | +90 days | 150,000,000,000,000 |
| 2 | 150,000 GNK | +180 days | 150,000,000,000,000 |
| 3 | 170,000 GNK | +270 days | 170,000,000,000,000 |
| **Total** | **970,000 GNK** | | **970,000,000,000,000** |

> 1 GNK = 1,000,000,000 ngonka (9 decimals)

## How It Works

1. Contract is instantiated with an **admin** (Gonka governance) and a **recipient** address
2. Four tranches are created automatically with hardcoded amounts and time offsets
3. After each tranche's unlock time passes, **anyone** can call `ReleaseTranche` to send GNK to the recipient
4. The admin can pause/resume the contract or perform an emergency withdrawal

## Messages

### Execute

| Message | Access | Description |
|---------|--------|-------------|
| `ReleaseTranche { tranche_id }` | Anyone | Release GNK for an unlocked tranche to the recipient |
| `Pause {}` | Admin | Pause all releases |
| `Resume {}` | Admin | Resume releases |
| `UpdateRecipient { recipient }` | Admin | Change recipient address |
| `WithdrawNativeTokens { amount, recipient }` | Admin | Withdraw specific amount of GNK |
| `EmergencyWithdraw { recipient }` | Admin | Withdraw all remaining GNK |

### Query

| Message | Description |
|---------|-------------|
| `Config {}` | Contract configuration (admin, recipient, denom, pause state, start time) |
| `Tranche { tranche_id }` | Single tranche details |
| `AllTranches {}` | All four tranches |
| `NativeBalance {}` | Contract's current GNK balance |

## Build

### Prerequisites

- Rust with `wasm32-unknown-unknown` target
- Docker (for optimized production build)

### Run Tests

```bash
cargo test
```

### Dev Build

```bash
cargo build --target wasm32-unknown-unknown --release
```

### Production Build (optimized)

```bash
docker run --rm -v "$(pwd)":/code \
  --mount type=volume,source="$(basename "$(pwd)")_cache",target=/target \
  --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
  cosmwasm/optimizer:0.17.0
```

Output: `artifacts/gonka_thesoul_vesting.wasm`

## Dependencies

- CosmWasm 3.0.x
- cw-storage-plus 3.0.x
- cw2 3.0.x

## License

All rights reserved. This contract is published for review and audit purposes.
