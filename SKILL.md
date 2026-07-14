---
name: ladon
description: Safely validate, operate, integrate, or recover Ladon's configuration-driven EVM, Bitcoin, and Solana address-pool daemon and its Rust library boundary. The public CLI has only check and pool.
---

# Ladon operational skill

## Purpose and non-goals

Ladon validates a universal TOML configuration and pre-derives EVM, Bitcoin, and Solana addresses into SQLite or PostgreSQL for a separate consumer. It is an address-pool service.

| Use this skill for | Do not use this skill for |
| --- | --- |
| Config validation, pool operation, database-consumer integration, recovery, or approved in-process library derivation. | Signing, broadcasting, RPC/network actions, wallet export/import, key backup, ad-hoc secret generation, or a CLI wrapper around library functions. |

The CLI and library are distinct boundaries. Operate pools through the CLI; use `ladon::derive(Params)` only in approved in-process code that can protect sensitive output.

## Command selection

| Goal | Select | Why |
| --- | --- | --- |
| Validate configuration and reference availability without database access | `check` | It loads configuration and checks environment/file references, then exits. |
| Create/maintain database-backed available addresses | `pool` | It connects, ensures schema, and runs the refill loop. |
| Obtain an address for an application | Consumer database transaction | The consumer atomically claims a persisted available row. |
| Derive in application memory | Rust library, not shell | `WalletOutput` and `KeyInfo` can contain private material. |

Never suggest, invoke, or invent other Ladon CLI commands.

## Prerequisites and features

| Requirement | Verification / action |
| --- | --- |
| Rust | Version 1.97 or later. |
| Default database | SQLite is enabled by default; use a durable writable parent directory. Ladon opens `sqlite://` storage read-write-create but does not create parent directories. |
| PostgreSQL database | Build `cargo build --locked --bin ladon --features pg` (or `postgres`). |
| Config path | Supply `--config FILE` or set `LADON_CONFIG`. |
| Service identity | It must read config and secret file if applicable, and write SQLite storage or reach PostgreSQL. |
| Container | The repository Dockerfile runs as non-root `ladon`, defaults to `pool`, and has a `check` healthcheck. Mount config read-only, inject secrets at runtime, and persist SQLite data. |

Feature map: `pool` enables the binary and daemon; default features are `pool` plus `sqlite`; `pg` aliases `postgres`; `full` enables both database backends.

## Safe workflow

1. Confirm the task is pool operation, consumer integration, or approved library use—not signing or secret handling outside an approved boundary.
2. Inspect only non-secret config references. Ensure SQLite's parent exists or PostgreSQL is reachable by the intended service identity.
3. Run `ladon --config FILE check` as that identity. Do not paste resolved environment values into a terminal transcript, issue, commit, or response.
4. Start `ladon --config FILE pool` under a supervisor after `check` succeeds.
5. Verify the consumer uses an atomic claim transaction and preserves every issued row/index.
6. On failure, use stderr plus database/service state to diagnose. Never manufacture rows, reset indexes, release an uncertain claim, or delete rows to refill.

## Exact command reference

```sh
ladon --config FILE check
ladon --config FILE pool
```

| Command | Option/environment fallback | Success output | Failure and lifecycle |
| --- | --- | --- | --- |
| `check` | `--config FILE`; otherwise `LADON_CONFIG` is required | One stdout line: `configuration valid: store_driver=<sqlite|postgres>, table=<name>, chains=<n>, pool_target=<n>` | Non-zero on load/validation/reference failure. It does not connect to the database or validate secret content. |
| `pool` | Same config resolution | No address output; tracing goes to stderr under `RUST_LOG` | Connect/schema errors before the loop terminate. It checks all chains immediately, then sleeps each interval. A failing chain tick is logged and retried next interval. |

Neither command should emit address, mnemonic, passphrase, xpriv, private key, WIF, or resolved database URL. `check` validates references only.

## Config editing, resolution, and secrets

Ladon expands `${NAME}` and `${NAME:-default}` before parsing. It permits unrelated root namespaces but strictly rejects unknown fields in Ladon-owned sections.

| Config area | Required rule |
| --- | --- |
| `[ladon]` | `store` names a selected `[stores.<id>]`. |
| `[ladon.derive.secret]` | One `env`, `xpriv_env`, or `file` source. |
| `[[ladon.derive.chains]]` | At least one; `name` is unique canonical `evm`, `btc`, or `solana`; `chain` references an existing `[chains.<id>]`. |
| `[ladon.pool]` | Required. `target`, `batch`, and `interval_secs` are positive; `threshold <= target`. Defaults: 1000, 200, 100, 10. |
| `[ladon.table]` | Optional. Default table is `derived_addresses`; default columns are `id`, `chain`, `address`, `path`, `index`, `is_used`, `created_at`. |
| SQLite store | `driver = "sqlite"`; URL is `sqlite://<path>` (loader also accepts a bare path). |
| PostgreSQL store | `driver = "postgres"`; `url` must be exactly a required single environment reference such as `${DATABASE_URL}`. No literal URL, inline credentials, or default expansion. `max_connections` defaults to 5. |

| Secret kind | Configuration | Validation / use |
| --- | --- | --- |
| `env` | `var`; optional `passphrase_var` | Named variables must exist for `check`; `pool` reads them when refilling. |
| `xpriv_env` | `var` | Named variable must exist for `check`; it contains the library's raw hexadecimal xpriv input. |
| `file` | `path`; optional `passphrase_var` | `check` opens the file; `pool` reads and trims its mnemonic when refilling. |

Never store secret values in TOML, source control, logs, chat, test fixtures, or command lines. PostgreSQL's URL rule is mandatory even though ordinary TOML expansion supports defaults.

## Pool, derivation, and database contracts

| Contract | Required behavior |
| --- | --- |
| Availability | Only `is_used IS NULL` rows count as available. New rows leave this nullable boolean unset. |
| Refill | Below threshold, derive to target in transactions of at most `batch`; at or above threshold, do nothing. |
| Index continuity | Use `start_index` only when no row exists for that canonical chain. Thereafter use persisted `MAX(index) + 1`, including used rows. Do not delete, reuse, backfill, or alter indexes. |
| Overflow | If the next derivation would pass `u32::MAX`, the tick fails; it never wraps. |
| Identifier safety | Table/column names are trusted deployment configuration only: non-empty ASCII identifiers starting with letter/`_`, then letters/digits/`_`. Values are bound; identifiers are dynamic SQL. |
| EVM | `m/44'/60'/account'/change/index`, EIP-55 address. |
| Bitcoin | `m/44'/0'/account'/change/index`, native SegWit P2WPKH; network is `bitcoin`, `testnet`, `signet`, or `regtest`. |
| Solana | `m/44'/501'/account'/change'/index'`, hardened SLIP-0010. Modes are `full`, `cold-export`, `hsm-sim`, `pda`; PDA uses `program_id` when supplied. |

The consumer, not Ladon, claims addresses. Claim one row and set it used in the same transaction. For default SQLite names:

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

Commit an empty result as an empty pool; roll back on pre-commit application failure. PostgreSQL consumers must use PostgreSQL transaction/locking claim SQL, not SQLite `BEGIN IMMEDIATE`, while retaining the conditional availability update and returned claimed row. Never use a read-then-update claim sequence.

## Outputs, errors, and recovery

| Condition | Meaning | Recovery |
| --- | --- | --- |
| `check` success line | Config and secret references are valid; database access remains untested. | Start `pool` with the same identity/environment. |
| Missing config, malformed TOML, unknown Ladon fields, bad store/chain reference, pool bounds, identifiers, or secret reference | `check` fails non-zero. | Correct the reference/configuration; rerun `check`. |
| PostgreSQL feature failure | Binary lacks `pg`/`postgres`. | Rebuild with the needed feature. |
| Startup database/schema error | `pool` exits before its loop. | Correct connection, permissions, SQLite parent, or schema issue; rerun `check`, then restart. |
| Per-chain tick failure | Error is logged; loop continues and retries that chain next interval. | Diagnose secret/database/derivation input without changing history. |
| Consumed rows below threshold | Expected: used rows do not count available. | Leave markers intact; Ladon continues from the highest index. |
| Uncertain consumer claim | Delivery may already have occurred. | Reconcile transaction/audit state before retry; never reissue that address. |

## Security, reliability guardrails, and prohibited actions

- Do not reveal, serialize, debug-format, log, commit, or transmit mnemonic, passphrase, xpriv, private key, WIF, resolved database URL, `WalletOutput`, or full `KeyInfo`.
- Do not alter derivation inputs, chain mappings, account/change, table identifiers, or database location against an existing pool without continuity review.
- Do not delete database rows, reset the database, mark a used row available, recycle indexes, fabricate pool rows, or use volume deletion as shutdown.
- Do not treat `hsm-sim` as an external HSM integration. Do not use encryption helpers as a secret-management system.
- Run under a supervisor with protected stderr logs; stop normally through the supervisor and retain durable database storage.
- Library users may retain only address/path/index data after derivation. `WalletOutput`/`KeyInfo` may contain private material; Solana xpub watch-only derivation is unsupported.

## Verification checklist

```sh
cargo fmt --check
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
```

| Check | Complete when |
| --- | --- |
| Command boundary | Only `check` or `pool` is selected, with `--config` or `LADON_CONFIG`. |
| Secret boundary | No resolved secret or database URL entered output, logs, config, or version control. |
| Configuration | Selected store and each shared chain reference exist; chain names are unique/canonical; identifiers and pool bounds validate. |
| Backend | SQLite storage is durable/writable, or PostgreSQL was built with `pg`/`postgres` and uses the required URL environment reference. |
| Consumer | Claim transaction atomically changes `is_used IS NULL` to `TRUE` and preserves all historic rows/indexes. |
| Runtime | `check` passes under the pool identity; supervisor handles process failure; tick errors are investigated without destructive recovery. |
