---
name: ladon
description: Operate or integrate Ladon's configuration-driven address-pool service when an agent needs to validate its universal TOML configuration, run or recover the pool daemon, consume its database-backed address pool, or use its Rust derivation library without exposing private material. Do not use this skill for ad-hoc wallet generation, decryption, signing, broadcasting, or network operations: the public CLI supports only `check` and `pool`.
---

# Ladon operational instructions

## Purpose and non-goals

Ladon validates a universal TOML configuration and pre-derives EVM, Bitcoin, and Solana addresses into a SQLite or PostgreSQL pool for a separate consumer. It is an address-pool daemon, not a signing service, wallet-export tool, key backup mechanism, database administration tool, or blockchain RPC client.

Use this skill when deploying, validating, operating, troubleshooting, or integrating the Ladon daemon or Rust library. Do not use it to request, generate, reveal, transmit, decrypt, encrypt, import, export, sign with, or broadcast from wallet secrets. Never put mnemonics, extended private keys, passphrases, private keys, WIFs, database URLs, or resolved environment values in chat, shell history, configuration, logs, tests, commits, or command output.

## Public CLI: check and pool only

The public CLI deliberately has exactly these operational commands:

```sh
ladon --config FILE check
ladon --config FILE pool
```

`--config FILE` is required unless `LADON_CONFIG` names the file. Always run `check` successfully before starting `pool`.

`check` loads and validates the configuration, including secret-reference availability, but does **not** connect to or validate the database. On success it writes exactly one non-secret summary to stdout in this form:

```text
configuration valid: store_driver=sqlite, table=derived_addresses, chains=2, pool_target=1000
```

The values vary with configuration. Errors, tracing, and pool logs go to stderr; a non-zero exit status is failure. `check` exits after its report. `pool` loads configuration, connects to the selected database, creates or migrates its schema, then runs until stopped. Connection or schema errors before the loop terminate `pool`; a failure while refilling one chain is logged and retried on that chain's next interval. Neither command emits addresses, private keys, mnemonics, passphrases, xprivs, WIFs, or resolved database URLs.

Do not suggest or invoke removed/nonexistent CLI commands such as `derive`, `generate`, `decrypt`, `encrypt`, `wallet`, `key`, `address`, `network`, `send`, `sign`, or `broadcast`. Do not turn library functions into shell commands.

## Build and feature prerequisites

The binary requires the `pool` feature, which is enabled by default, and Rust 1.97 or later. Default builds include SQLite support. PostgreSQL requires building with either `pg` or `postgres`; the project alias `pg` enables `postgres`.

```sh
cargo build --locked --bin ladon
cargo build --locked --bin ladon --features pg
```

Use a writable, durable parent directory for a SQLite database before starting the daemon; the SQLite URL is opened as read-write-create, but Ladon does not create missing parent directories. The service identity must be able to read the config and secret file (if used) and write the SQLite directory, or reach PostgreSQL. A PostgreSQL configuration on a binary built without `pg`/`postgres` fails during config loading with an instruction to recompile with one of those features.

## Universal configuration

Ladon reads its owned namespace from a universal TOML document and ignores unrelated root namespaces. The configuration must contain:

- `[ladon]` with `store`, which names one selected `[stores.<id>]` entry.
- `[ladon.derive]`, `[ladon.derive.secret]`, and at least one `[[ladon.derive.chains]]` entry.
- `[ladon.pool]`.
- `[stores.<id>]` for the selected `ladon.store`.
- A matching `[chains.<id>]` for every `chain` referenced by `[[ladon.derive.chains]]`.

`[ladon.table]` and `[ladon.table.columns]` are optional as a unit: omitted values use the `derived_addresses` table and `id`, `chain`, `address`, `path`, `index`, `is_used`, and `created_at` columns. If customized, provide a valid table configuration and do not use unknown fields in Ladon-owned sections; those sections are strictly parsed.

Each local derivation chain uses canonical `name = "evm"`, `"btc"`, or `"solana"` only—CAIP-2 values and aliases are not chain names. Its `chain` is the key of the corresponding shared `[chains.<id>]` entry. `account`, `change`, and `start_index` default to zero. Chain names must not repeat.

Pool settings default to `target = 1000`, `threshold = 200`, `batch = 100`, and `interval_secs = 10`. Explicit or defaulted values must satisfy: target, batch, and interval are positive; threshold is not greater than target. The `network` setting for Bitcoin defaults to `bitcoin` and accepts `bitcoin`, `testnet`, `signet`, or `regtest`. The Solana `solana_mode` defaults to `full`; supported modes are `full`, `cold-export`, `hsm-sim`, and `pda`. `program_id` is used for PDA mode.

Environment expansion in TOML supports `${NAME}` and `${NAME:-default}` generally. Never use a default for a PostgreSQL URL.

## Secret sources and handling

`[ladon.derive.secret]` is a reference, never a secret value. Supported sources are:

- `kind = "env"` with `var` naming an environment variable containing a BIP-39 mnemonic; optional `passphrase_var` names a required environment variable when configured.
- `kind = "xpriv_env"` with `var` naming an environment variable containing the raw hexadecimal xpriv expected by the library.
- `kind = "file"` with `path` to a protected plaintext mnemonic file; optional `passphrase_var` is an environment-variable name.

`check` verifies that environment variables exist or that the file can be opened; it does not print or validate the secret contents. `pool` resolves the source only when a refill is needed. Protect secret files with least-privilege ownership and permissions, pass secrets through the service manager or an approved secret store, and redact variable values from diagnostics. Do not inline a mnemonic, xpriv, or passphrase in TOML.

For PostgreSQL, `[stores.<id>]` must use `driver = "postgres"` and set `url` to exactly one required environment reference such as `${DATABASE_URL}`. Inline credentials, literal URLs, and `${NAME:-default}` are rejected. `max_connections` defaults to 5 for PostgreSQL. SQLite uses `driver = "sqlite"` and a `sqlite://` URL; its effective connection limit is one.

## Derivation semantics and index continuity

Ladon uses BIP-44-style bases followed by the stored index:

- EVM: `m/44'/60'/account'/change/index` using secp256k1; addresses are EIP-55 checksummed.
- Bitcoin: `m/44'/0'/account'/change/index` using secp256k1 and the configured Bitcoin network; addresses are native SegWit P2WPKH.
- Solana: `m/44'/501'/account'/change'/index'` using hardened SLIP-0010 Ed25519 derivation.

`start_index` is used only when that canonical chain has no persisted rows. Its default is `0`. Once any row exists, Ladon derives from the highest persisted index plus one, regardless of availability or `start_index`; it fails rather than wrapping after `u32::MAX`. Never delete, alter, reuse, or backfill indexes merely to make addresses appear available.

For Solana, `cold-export` records the public address while the library marks private output hidden; `pda` derives a receive-only PDA using the configured program ID (or the library's default when empty). The daemon persists only address, path, index, chain, identifier, and timestamp—not library key output. Do not infer that `hsm-sim` adds external HSM integration; it is a library mode name.

## Pool lifecycle, schema, and consumer claiming

On startup `pool` creates the configured table if absent and ensures the nullable boolean `is_used` column exists. The schema contains a text primary-key `id`, text `chain`, `address`, and `path`, integer `index`, nullable boolean `is_used`, and text ISO-8601 `created_at`, mapped through the configured identifiers. New rows leave `is_used` as `NULL`; available-count queries use `is_used IS NULL` for each canonical chain.

Each loop immediately checks every configured chain, then sleeps for `interval_secs`. When available rows are below `threshold`, it refills to `target` in batches no larger than `batch`; when the count is at least threshold it does nothing. Each batch is inserted transactionally. A row marked `TRUE` is consumed and will not count as available.

The consuming service—not Ladon—must atomically select one available row and mark that same row `is_used = TRUE` within its database transaction. It must never mark a row available again, delete it to recycle an index, or claim a row with a non-atomic read-then-update sequence. Design the consumer's transaction and database-specific locking so concurrent consumers cannot receive the same row. Preserve all rows so the daemon's highest-index query maintains continuity.

Table and column identifiers are Ladon's dynamic-SQL trust boundary. They must be non-empty ASCII identifiers beginning with a letter or underscore and containing only ASCII letters, digits, and underscores. Treat these names as trusted deployment configuration only: never accept them from a request, tenant, user, or SQL fragment. Database values are bound parameters; identifiers are interpolated only after this validation.

## Deployment and shutdown

Run `check` under the same service account and environment used by `pool`, then start `pool` under a supervisor that restarts failures. Keep `RUST_LOG` at an appropriate non-secret verbosity and send stderr to protected operational logs. Container deployments must mount the config read-only, supply secrets at runtime, and mount persistent writable storage for SQLite. System service deployments should use a protected environment file or secret mechanism rather than placing values in the unit file.

`pool` runs until the process is stopped. Stop it through the supervisor or send normal process termination, allow the process to exit, and do not kill the database or remove its volume as a shutdown procedure. Before changing derivation inputs, chain account/change settings, canonical chain mappings, table identifiers, or database location, stop the daemon and assess existing pool continuity; changing them against an existing pool can mix incompatible derivation state. Do not rotate secrets or rebuild an empty database casually when previously issued addresses may still be in use.

## Troubleshooting and recovery

- Missing `--config`/`LADON_CONFIG`, malformed TOML, unknown Ladon keys, missing store/chain references, invalid pool bounds, invalid identifiers, or unavailable secret references: correct the configuration or reference, then rerun `check`.
- `check` success does not prove database access. For connection, permission, missing SQLite-parent, PostgreSQL feature, or schema errors, correct the runtime/build/storage issue and restart `pool` after a successful `check`.
- A pool tick error is logged per chain and retried on the next interval. Investigate the stderr error and underlying database or secret-source availability; do not manually fabricate rows or reset indices.
- If rows were consumed but the pool remains below threshold, leave their `is_used` markers intact and let the daemon refill from the persisted maximum index.
- If a consumer has an uncertain claim outcome, reconcile against the same database transaction/audit state before retrying. Never release or reissue a possibly delivered address.
- If `start_index` seems ignored, inspect existing rows for that canonical chain: it is only an empty-chain bootstrap value.

## Rust library guardrails

The library API is not the public operational CLI. `ladon::derive(Params)` and lower-level derivation functions return `WalletOutput`/`KeyInfo` structures that can contain mnemonic, passphrase, extended private key, private-key, or WIF fields. Use them only in approved in-process secret-handling code; never serialize, debug-format, log, return, or place those objects in telemetry. Prefer the daemon for address-pool production workflows.

If library derivation is necessary, accept only canonical chains via `canonical_chain`, set explicit indexes or the intended index set, and retain only the required public address/path/index data. Do not use `encrypt_data`/`decrypt_data` as a replacement for an approved secret-management system, and do not create a CLI wrapper around them. Watch-only xpub derivation is unsupported for Solana; xpriv and mnemonic inputs remain sensitive even when only addresses are retained.

## Validation and final checklist

Run the repository checks after changes to Ladon code or configuration-facing behavior:

```sh
cargo fmt --check
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
```

Before declaring an operation complete, confirm:

1. The command is only `check` or `pool`, with `--config FILE` or `LADON_CONFIG`.
2. `check` succeeded under the pool service's identity without exposing a resolved secret.
3. Required universal sections, selected store, and every shared chain reference are present.
4. PostgreSQL is built with `pg`/`postgres` and its URL is a required environment reference; or SQLite storage and its parent directory are durable and writable.
5. Identifiers are trusted ASCII identifiers, pool bounds are valid, and canonical chain names are unique.
6. The consumer atomically claims `is_used IS NULL` rows and permanently marks them used.
7. Existing rows and their highest indexes are preserved; no secret, address, private material, or database URL was added to output, logs, or version control.
