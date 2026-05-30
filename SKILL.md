---
name: ladon
description: >
  Multi-chain HD wallet CLI, library, and address-pool daemon for EVM, Bitcoin, and Solana.
  Use this skill whenever the user needs to derive cryptocurrency addresses, manage HD wallets,
  encrypt/decrypt wallet files, or run an address-pool daemon. Covers key derivation (BIP-32/44,
  SLIP-0010), mnemonic generation, xpub/xpriv watch-only workflows, batch address generation,
  and persistent address-pool management with SQLite or Postgres. Trigger on mentions of
  wallet derivation, multi-chain addresses, HD wallet, BIP-39 mnemonics, address pools,
  EVM/BTC/Solana key generation, or any task involving cryptocurrency address management.
---

# Ladon — Multi-Chain HD Wallet

> *Like the hundred-headed serpent of Greek myth, Ladon generates as many addresses as you need.*

Ladon is a fast, minimal, multi-chain HD wallet CLI, Rust library, and address-pool daemon. It supports EVM, Bitcoin, and Solana chains with BIP-32/44 and SLIP-0010 derivation.

## Install

```sh
cargo install ladon
```

## Modes

Ladon has three sub-commands:

| Sub-command | Purpose |
|-------------|---------|
| `derive`    | Derive one or more addresses and print to stdout |
| `decrypt`   | Decrypt an encrypted wallet file |
| `pool`      | Run the address-pool daemon (requires `[database]` in config) |

---

## CLI Usage

### Configuration

All settings live in `Config.toml` (or any path passed with `--config` / `-C`). Per-flag overrides are available on `derive` for ad-hoc use.

```sh
ladon --config /etc/ladon/Config.toml pool
```

### Derive

Output goes to stdout. Redirect it to whatever format you need:

```sh
# JSON (default) — pipe or redirect
ladon derive --chain evm --num 5
ladon derive --chain evm --num 20 > wallet.json

# CSV
ladon derive --chain evm --num 20 --format csv > wallet.csv

# Plain text (one address per line)
ladon derive --chain solana --num 10 --format text > addresses.txt

# Encrypted output
ladon derive --chain evm --num 5 --encrypt --password "secret" > wallet.enc

# Bitcoin from existing mnemonic
ladon derive --chain btc --mnemonic "word1 ... word12"

# Specific indexes or ranges
ladon derive --chain evm --indexes "0,5,22-44,55,66-109"

# Watch-only from xpub
ladon derive --chain evm --xpub xpub6C...

# From xpriv
ladon derive --chain evm --xpriv xprv9s...
```

#### Key Flags

| Flag | Default | Description |
|------|---------|-------------|
| `--chain` / `-c` | `evm` | `evm`, `btc`, `solana` |
| `--num` / `-n` | `1` | Number of addresses |
| `--index` / `-i` | — | Single specific index |
| `--indexes` | — | Comma-separated indexes/ranges |
| `--account` | `0` | BIP-44 account |
| `--change` | `0` | BIP-44 change |
| `--network` | `bitcoin` | Bitcoin network |
| `--passphrase` | `""` | BIP-39 passphrase |
| `--strength` / `-s` | `12` | Mnemonic word count |
| `--solana-mode` | `full` | `full`, `cold-export`, `hsm-sim`, `pda` |
| `--program-id` | — | Base58 program ID for PDA mode |
| `--xpub` | — | Watch-only from xpub |
| `--xpriv` | — | Derive from xpriv |
| `--format` / `-f` | `json` | `json`, `csv`, `text` |
| `--encrypt` | false | Encrypt output |
| `--password` | — | Encryption password |

### Decrypt

```sh
ladon decrypt wallet.enc --password "secret"
ladon decrypt wallet.enc --password "secret" > wallet.json
```

---

## Address-Pool Daemon

The `pool` sub-command runs a long-lived service that:

1. Connects to a SQLite or Postgres database.
2. Polls the pool table on a configurable interval.
3. Derives and inserts new addresses when the count drops below `[pool].threshold`.
4. Keeps the total at `[pool].target` addresses per chain.

Your application removes rows from the pool table as it assigns addresses to users.

### Example Config.toml (pool mode)

```toml
# ── Derivation ──────────────────────────────────────────────────────────────────
[derive]

# How to obtain the master secret.  Supported "kind" values:
#   "env"        — mnemonic from an environment variable
#   "xpriv_env"  — raw hex xpriv from an environment variable
#   "file"       — mnemonic read from a plain-text file
[derive.secret]
kind = "env"
var  = "LADON_MNEMONIC"
# Optional: BIP-39 passphrase read from a separate env var
# passphrase_var = "LADON_PASSPHRASE"

# One [[derive.chains]] section per blockchain.
[[derive.chains]]
name    = "evm"
account = 0
change  = 0

[[derive.chains]]
name        = "solana"
account     = 0
change      = 0
solana_mode = "cold-export"   # "full" | "cold-export" | "hsm-sim" | "pda"
# program_id = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"

# [[derive.chains]]
# name    = "btc"
# account = 0
# change  = 0
# network = "bitcoin"   # "bitcoin" | "testnet" | "signet" | "regtest"

# ── Pool daemon ─────────────────────────────────────────────────────────────────
[pool]
target        = 1000   # keep this many addresses in the pool at all times
threshold     = 200    # start refilling when the count drops below this
batch         = 100    # derive this many at a time
interval_secs = 10     # poll the database every N seconds

# ── Database — SQLite ───────────────────────────────────────────────────────────
[database.sqlite]
path = "data/addresses.db"

[database.sqlite.table]
name = "derived_addresses"

[database.sqlite.table.columns]
id         = "id"
chain      = "chain"
address    = "address"
path       = "path"
index      = "index"
created_at = "created_at"

# ── Database — Postgres (comment out the sqlite section above if using this) ────
# [database.postgres]
# url      = "postgres://user:password@localhost:5432/ladon"
# # Alternatively, set the env var DATABASE_URL and omit `url` here.
# # url_env  = "DATABASE_URL"
# pool_size = 5
#
# [database.postgres.table]
# name = "derived_addresses"
#
# [database.postgres.table.columns]
# id         = "id"
# chain      = "chain"
# address    = "address"
# path       = "path"
# index      = "index"
# created_at = "created_at"
```

---

## Docker

Published images are available from GitHub Container Registry. The image does
not include a config file. Mount your environment-specific `Config.toml` at
runtime and provide secrets through environment variables.

```sh
docker run --rm \
  -e LADON_MNEMONIC="word1 word2 ... word12" \
  -e DATABASE_URL="postgres://user:password@host:5432/ladon" \
  -v "$PWD/Config.toml:/app/Config.toml:ro" \
  ghcr.io/melonask/ladon:latest
```

For SQLite-backed pool mode, also mount a writable data directory:

```sh
docker run --rm \
  -e LADON_MNEMONIC="word1 word2 ... word12" \
  -v "$PWD/Config.toml:/app/Config.toml:ro" \
  -v "$PWD/data:/app/data" \
  ghcr.io/melonask/ladon:latest
```

The default container command is `pool`. Override it to run other commands:

```sh
docker run --rm ghcr.io/melonask/ladon:latest derive --chain evm --num 5
docker run --rm \
  -v "$PWD/prod.toml:/config/Config.toml:ro" \
  ghcr.io/melonask/ladon:latest --config /config/Config.toml pool
```

```sh
# Start the pool daemon with Postgres using deploy/docker-compose.yml
export LADON_IMAGE="ghcr.io/melonask/ladon:latest"
docker compose -f deploy/docker-compose.yml up -d
```

Set `LADON_IMAGE` and `LADON_MNEMONIC` in the environment or in `deploy/.env`.
The compose file points `DATABASE_URL` at its bundled Postgres service. Edit it
if you want to use an external database. The container uses
`restart: unless-stopped` so it recovers automatically.

## Systemd (bare-metal)

```sh
sudo cp deploy/ladon.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now ladon
```

Store secrets in `/etc/ladon/env`:

```
LADON_MNEMONIC=word1 word2 ... word12
DATABASE_URL=postgres://ladon:secret@localhost/ladon
RUST_LOG=ladon=info
```

---

## Library Usage (Rust)

```rust
use ladon::{derive, decrypt_data, encrypt_data, Params, WalletOutput};

let wallet: WalletOutput = derive(Params {
    chain: "evm".into(),
    num: 5,
    ..Default::default()
})?;

for key in &wallet.keys {
    println!("{}: {}", key.index, key.address);
}
```

---

## Chains and Derivation Paths

| Chain | Default path |
|-------|-------------|
| EVM | `m/44'/60'/0'/0/*` |
| Bitcoin | `m/44'/0'/0'/0/*` |
| Solana | `m/44'/501'/0'/0'` (hardened, SLIP-0010) |

---

## Security Notes

- Private keys are **zeroized on drop**.
- Use `--solana-mode cold-export` or `--encrypt` when storing derive output.
- `--password` on the CLI is visible in shell history. Prefer environment-specific secret management (`LADON_MNEMONIC`, vault agents, etc.) in automation.
- Ed25519 derivation follows **SLIP-0010** (all path segments hardened).
- In the pool daemon, the mnemonic is never written to disk — it lives only in the process environment.
