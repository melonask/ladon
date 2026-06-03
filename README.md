# ladon

<img align="right" src="https://raw.githubusercontent.com/melonask/ladon/refs/heads/main/logo.svg" alt="Fast, minimal, multi-chain HD wallet CLI and library — EVM, Bitcoin, Solana" width="160" />

> *Like the hundred-headed serpent of Greek myth, Ladon generates as many addresses as you need.*

Fast, minimal, multi-chain HD wallet CLI, library, and address-pool daemon — EVM, Bitcoin, Solana.

---

## Install

```sh
cargo install ladon
```

---

## Modes

ladon has three sub-commands:

| Sub-command | Purpose |
|-------------|---------|
| `derive`    | Derive one or more addresses and print to stdout |
| `decrypt`   | Decrypt an encrypted wallet file |
| `pool`      | Run the address-pool daemon (requires `[database]` in config) |

---

## CLI

### Configuration file

All settings live in `Config.toml` (or any path passed with `--config`/`-C`).
Per-flag overrides are available on `derive` for ad-hoc use.

```sh
ladon --config /etc/ladon/Config.toml pool
```

See [Config.toml](./Config.toml) for a fully annotated example.

---

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

#### Key flags

| Flag | Default | Description |
|------|---------|-------------|
| `--chain` / `-c` | `evm` | `evm`/`eip155[:id]`, `btc`/`bip122[:id]`, `solana[:id]` |
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

## Address-pool daemon

The `pool` sub-command runs a long-lived service that:

1. Connects to a SQLite or Postgres database.
2. Polls the pool table on a configurable interval.
3. Derives and inserts new addresses when the count drops below `[pool].threshold`.
4. Keeps the total at `[pool].target` addresses per chain.

Available rows have `is_used IS NULL`. When assigning an address, your application
should retrieve the oldest available row first, ordered by ascending `index` per chain.
Then either set `is_used = true` or delete the assigned row.

Marking assigned rows with `is_used = true` is the safest mode: used rows no longer
count toward the available pool, but they still preserve the maximum generated `index`.
If your application deletes assigned rows instead, at least one highest-index row must
remain in the table for each chain. Otherwise Ladon cannot distinguish a never-filled
pool from a fully-consumed pool and will restart generation at the chain's configured
`start_index` value, which defaults to `0`.

Set `start_index` on a `[[derive.chains]]` entry to control the first index generated
for a brand-new chain pool. Once any row exists for that chain, Ladon always continues
from `MAX(index) + 1`.

### Example Config.toml (pool mode)

```toml
[derive.secret]
kind = "env"
var  = "LADON_MNEMONIC"

[[derive.chains]]
name = "evm"
start_index = 1

[pool]
target        = 1000
threshold     = 200
batch         = 100
interval_secs = 10

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
is_used    = "is_used"
created_at = "created_at"
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
if you want to use an external database.
The container uses `restart: unless-stopped` so it recovers automatically.

---

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

## Library

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

## Chains & derivation paths

| Chain | Default path |
|-------|-------------|
| EVM | `m/44'/60'/0'/0/*` |
| Bitcoin | `m/44'/0'/0'/0/*` |
| Solana | `m/44'/501'/0'/0'` (hardened, SLIP-0010) |

---

## Security notes

- Private keys are **zeroized on drop**.
- Use `--solana-mode cold-export` or `--encrypt` when storing derive output.
- `--password` on the CLI is visible in shell history. Prefer environment-specific
  secret management (`LADON_MNEMONIC`, vault agents, etc.) in automation.
- Ed25519 derivation follows **SLIP-0010** (all path segments hardened).
- In the pool daemon, the mnemonic is never written to disk — it lives only
  in the process environment.

---

## License

MIT
