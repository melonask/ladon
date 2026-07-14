# ladon

<img align="right" src="https://raw.githubusercontent.com/melonask/ladon/refs/heads/main/logo.svg" alt="Fast, minimal, multi-chain HD wallet CLI and library — EVM, Bitcoin, Solana" width="200" />

> *Like the hundred-headed serpent of Greek myth, Ladon generates as many addresses as you need.*

Ladon is a configuration-driven address-pool service for EVM, Bitcoin, and Solana. It validates a universal TOML configuration and pre-derives addresses into SQLite or PostgreSQL for another service to consume.

[Documentation](https://melonask.github.io/ladon/) · [Getting started](https://melonask.github.io/ladon/getting-started/) · [Configuration](https://melonask.github.io/ladon/getting-started/configuration) · [Repository](https://github.com/melonask/ladon)

## At a glance

| Area | Behavior |
| --- | --- |
| Public CLI | `check` validates configuration and secret references; `pool` maintains address rows. |
| Chains | Canonical local names are `evm`, `btc`, and `solana`; each maps to a shared `[chains.<id>]` entry. |
| Stores | SQLite is enabled by default; PostgreSQL requires the `pg` or `postgres` feature. |
| Pool availability | A row is available only while `is_used IS NULL`; consumers permanently claim it by setting `is_used = TRUE`. |
| Secret boundary | TOML contains references, not mnemonic, xpriv, passphrase, or PostgreSQL URL values. |

## Requirements, installation, features, and container

Ladon requires Rust 1.97 or later. Build the default SQLite-enabled binary:

```sh
cargo build --locked --release --bin ladon
```

| Feature | Effect |
| --- | --- |
| `pool` | Enables the binary, runtime, database support, and pool daemon; enabled by default. |
| `sqlite` | SQLite backend; enabled by default. |
| `postgres` | PostgreSQL backend. |
| `pg` | Alias for `postgres`. |
| `full` | Enables SQLite and PostgreSQL. |

For PostgreSQL, build with `cargo build --locked --release --bin ladon --features pg`.

The repository includes a `Dockerfile` that builds with `full`, runs as the unprivileged `ladon` user, sets `LADON_CONFIG=/etc/ladon/Config.toml`, and starts `pool` by default. Its `check` healthcheck also validates the configured secret reference, so the referenced mnemonic environment variable must be injected into the container. `deploy/docker-compose.yml` is an operational example. Mount configuration read-only, inject secrets at runtime, and use durable writable storage when SQLite is selected.

## Quick start

Create a writable parent directory for the SQLite database, write a universal configuration, and provide the referenced secret only in the runtime environment:

```sh
mkdir -p data/ladon
cat > Config.toml <<'EOF'
[stores.ladon]
driver = "sqlite"
url = "sqlite://data/ladon/addresses.db"

[chains.ethereum-mainnet]
caip2 = "eip155:1"

[ladon]
store = "ladon"

[ladon.derive.secret]
kind = "env"
var = "LADON_MNEMONIC"

[[ladon.derive.chains]]
name = "evm"
chain = "ethereum-mainnet"

[ladon.pool]
target = 1000
threshold = 200
batch = 100
interval_secs = 10
EOF
read -r -s 'LADON_MNEMONIC?Enter the approved BIP-39 mnemonic: '
export LADON_MNEMONIC
ladon --config Config.toml check
ladon --config Config.toml pool
```

The silent `read` command above is for an interactive zsh session and accepts the real mnemonic supplied by an approved secret manager without echoing it. Run `check` and `pool` as the same service identity. Do not put the mnemonic or its resolved value in TOML, shell history, logs, source control, or command output. The supplied [Config.toml](Config.toml) is a complete multi-chain example.

## CLI reference

```
ladon --config FILE check
ladon --config FILE pool
```

`--config FILE` is the only CLI option. It falls back to `LADON_CONFIG`; one of them is required. The public CLI has exactly the commands below.

| Command | Work | Stdout | Exit behavior |
| --- | --- | --- | --- |
| `check` | Loads the config; validates Ladon-owned fields, store and chain references, pool bounds, identifiers, feature compatibility, and secret-reference availability. It does not connect to the database or validate secret contents. | Exactly `configuration valid: store_driver=<sqlite|postgres>, table=<name>, chains=<n>, pool_target=<n>` on success. | Returns success after the report; configuration or reference failures are non-zero. |
| `pool` | Loads config, connects, creates the configured table when absent, attempts the `is_used` column migration, then refills configured chains until stopped. | No address output. | Connection or schema failure before the loop is non-zero. A per-chain tick failure is logged and that chain is retried next interval. |

Tracing uses `RUST_LOG`; tracing and errors are written to stderr. Neither command intentionally prints mnemonic, passphrase, xpriv, private keys, WIFs, addresses, or resolved PostgreSQL URLs.

## Configuration model

Ladon expands `${NAME}` (required) and `${NAME:-default}` before TOML parsing. It reads and strictly parses its `[ladon]` namespace while allowing unrelated root namespaces. `ladon.store` selects a shared store, and every local chain reference must exist in `[chains]`.

### Required structure

| Location | Required fields / rule |
| --- | --- |
| `[ladon]` | `store`, naming `[stores.<id>]`. |
| `[ladon.derive.secret]` | A supported secret source. |
| `[[ladon.derive.chains]]` | At least one entry; its `chain` must name `[chains.<id>]`. |
| `[ladon.pool]` | Required; omitted individual settings use defaults. |
| `[stores.<id>]` | Selected store with `driver` and `url`. |
| `[chains.<id>]` | Required for each local chain reference; Ladon checks the key exists. |

`[ladon.table]` may be omitted, in which case the table is `derived_addresses` with columns `id`, `chain`, `address`, `path`, `index`, `is_used`, and `created_at`. If configured, it contains `name` and `[ladon.table.columns]`; column fields default to those same names.

### Stores, pool, and chains

| Setting | Values / default | Constraint |
| --- | --- | --- |
| `[stores.<id>].driver` | `sqlite` or `postgres` | PostgreSQL needs a binary built with `pg` or `postgres`. |
| SQLite `url` | `sqlite://<path>` (the prefix is optional to the loader) | Opens read-write-create with one connection; Ladon does not create missing parent directories. |
| PostgreSQL `url` | Exactly `${UPPERCASE_NAME}` | Required single environment reference; literals and `${NAME:-default}` are rejected. |
| `max_connections` | PostgreSQL default `5` | Used by PostgreSQL; SQLite uses one connection. |
| `target` | `1000` | Must be positive. |
| `threshold` | `200` | Must not exceed `target`. |
| `batch` | `100` | Must be positive. |
| `interval_secs` | `10` | Must be positive. |
| chain `name` | `evm`, `btc`, `solana` | Canonical and unique; CAIP-2 values and aliases are rejected. |
| `account`, `change`, `start_index` | `0` | `start_index` applies only to a chain with no persisted rows. |
| Bitcoin `network` | `bitcoin` | `bitcoin`, `testnet`, `signet`, or `regtest`; other values fall back to `bitcoin` in library derivation. |
| Solana `solana_mode` | `full` | `full`, `cold-export`, `hsm-sim`, or `pda`; `program_id` is parsed for `pda`. |

Table and column identifiers are a dynamic-SQL trust boundary. They must be non-empty ASCII identifiers beginning with a letter or `_`, followed only by ASCII letters, digits, or `_`. They are deployment-controlled names, never request data.

### Secret sources

| `kind` | Required fields | Runtime behavior |
| --- | --- | --- |
| `env` | `var`; optional `passphrase_var` | Reads a BIP-39 mnemonic and optional passphrase from named environment variables. |
| `xpriv_env` | `var` | Reads the raw hexadecimal xpriv expected by the library. |
| `file` | `path`; optional `passphrase_var` | Reads and trims a plaintext mnemonic file; the optional passphrase remains an environment variable. |

`check` verifies referenced environment variables exist or the file can be opened. `pool` resolves its secret when a refill is needed. Use a protected service-manager or secret-store interface and least-privilege file permissions.

## Pool lifecycle, derivation, and consumer claims

On each loop, Ladon checks each configured canonical chain, then sleeps for `interval_secs`. If available rows are below `threshold`, it derives enough rows to reach `target`, inserting batches of at most `batch` rows transactionally. New rows have a ULID text `id`, canonical `chain`, address, path, `index`, timestamp, and `is_used = NULL`.

| Chain | Stored-index derivation path |
| --- | --- |
| EVM | `m/44'/60'/account'/change/index`; EIP-55 address. |
| Bitcoin | `m/44'/0'/account'/change/index`; native SegWit P2WPKH on the configured network. |
| Solana | `m/44'/501'/account'/change'/index'`; hardened SLIP-0010 Ed25519. |

`start_index` is used only when no row exists for that canonical chain. Otherwise the next index is the persisted maximum plus one, including used rows. Exhaustion at `u32::MAX` fails rather than wrapping. Preserve rows and indexes: deleting, reusing, or backfilling an index can make a later derivation collide with an issued address.

The consumer owns claiming. It must select an available row and mark that same row used in one database transaction. For the default SQLite identifiers, this is a copyable single-consumer/concurrent-writer-safe pattern:

```sql
BEGIN IMMEDIATE;
UPDATE derived_addresses
SET is_used = TRUE
WHERE id = (
  SELECT id FROM derived_addresses
  WHERE chain = :chain AND is_used IS NULL
  ORDER BY "index", id
  LIMIT 1
)
AND is_used IS NULL
RETURNING id, chain, address, path, "index", created_at;
COMMIT;
```

If the update returns no row, commit and report the pool empty. Roll back on an application failure before commit. PostgreSQL consumers must use PostgreSQL transaction and locking/claim SQL rather than SQLite's `BEGIN IMMEDIATE`; preserve the same conditional `is_used IS NULL` update and return the claimed row before committing. Adapt configured identifiers only after the same identifier validation.

## Outputs, errors, and troubleshooting

| Symptom or output | Meaning | Safe response |
| --- | --- | --- |
| `configuration valid: ...` | `check` completed without exposing secret values. | Start `pool` under the same identity/environment. |
| Missing `--config` / `LADON_CONFIG` | No configuration path was supplied. | Supply one. |
| Missing environment reference or unreadable secret file | `check` cannot verify the configured reference. | Correct the service environment or permissions; do not print the value. |
| PostgreSQL feature error | The selected store requires `pg`/`postgres`. | Rebuild with that feature. |
| SQLite connection failure | The path, parent permissions, or storage are unsuitable. | Create and protect the parent directory, then restart. |
| Schema failure | Table creation or `is_used` migration failed. | Fix database permissions/schema compatibility; do not fabricate rows. |
| `Pool tick failed` | One chain failed during a loop. Other chains continue; this chain retries next interval. | Investigate stderr and the underlying secret/database condition without changing indexes. |
| Consumer uncertain whether a claim committed | The address may have been delivered. | Reconcile against the database transaction/audit record; never reissue it. |

## Public Rust library API

The library boundary is separate from the operational CLI. `derive(Params)` returns `WalletOutput`, containing `keys: Vec<KeyInfo>`. `KeyInfo` carries public `index`, `path`, `public_key`, and `address`, but output structures can also carry mnemonic, passphrase, xpriv, private key, and WIF material. Keep the object in approved in-process secret handling and retain only required public fields.

`Params` selects canonical `chain`, source (`mnemonic`, `xpriv`, or `xpub`), account/change, optional `index` or `indexes`, and chain-specific settings. `indexes` accepts comma-separated indexes and inclusive ranges and supersedes `index` and `num`.

```rust
use ladon::{derive, Params};

let mnemonic = std::env::var("LADON_MNEMONIC")?;
let wallet = derive(Params {
    chain: "evm".into(),
    mnemonic: Some(mnemonic),
    index: Some(0),
    ..Default::default()
})?;
let public = (wallet.keys[0].index, wallet.keys[0].path.clone(), wallet.keys[0].address.clone());
drop(wallet); // do not serialize, debug-format, or log the full output
println!("index={} path={} address={}", public.0, public.1, public.2);
# Ok::<(), anyhow::Error>(())
```

The test suite covers canonical names, explicit indexes, EVM output, and Solana modes. Do not expose library operations as shell commands. Solana does not support watch-only xpub derivation.

## Security, reliability, and deployment

- Never log, commit, transmit, or inline mnemonic, passphrase, xpriv, private key, WIF, resolved database URL, or full `WalletOutput`/`KeyInfo`.
- Protect configuration and secret files; run `check` before `pool` under the production service identity.
- Supervise `pool` and retain durable database storage. A startup connection/schema error stops the process; tick errors are logged and retried per chain.
- Do not reset a pool, alter derivation inputs, remap chains, change account/change values, or change table/database location without assessing existing issued addresses and continuity.
- Stop through the supervisor; do not use database deletion or volume removal as a shutdown procedure.

## Development verification

```sh
cargo fmt --check
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
```

## License

[MIT](LICENSE)
