---
name: ladon
description: Operate Ladon's configuration-driven check and address-pool commands safely.
---

# Ladon operational instructions

Use Ladon only through `check` and `pool`; do not suggest removed ad-hoc derivation, decryption, or network commands. The Rust library may derive wallets, but never expose private material in output, logs, or files unless the user controls a secure local destination.

## Safe workflow

1. Require `--config FILE` or `LADON_CONFIG`.
2. Run `ladon --config FILE check` before `pool` and report only its non-secret summary.
3. Run `ladon --config FILE pool` only when a long-lived pool is intended.
4. `check` success is stdout; tracing and errors are stderr; non-zero exits are failures.

## Universal configuration and secrets

Require `[ladon]`, `[ladon.derive.secret]`, `[[ladon.derive.chains]]`, `[ladon.pool]`, the selected `[stores.<id>]`, and every referenced `[chains.<id>]`. Chain names are exactly `evm`, `btc`, and `solana`; target, batch, and interval must be positive and threshold cannot exceed target.

Secrets are references, never values: use `env`, `xpriv_env`, or protected `file` sources. Keep mnemonics, xprivs, passphrases, and PostgreSQL URLs out of chat, shell history, TOML, logs, and commits. PostgreSQL URLs must be a required environment reference such as `${DATABASE_URL}`; inline credentials and defaults are invalid.

Table and column identifiers are the dynamic-SQL trust boundary. Use plain ASCII identifiers beginning with a letter or underscore; never supply SQL fragments.

## Guardrails

Do not alter, delete, or reuse rows to simulate availability. Atomically claim available rows and set `is_used = true` so the highest index remains persisted. Do not claim that `check` verifies database connectivity. Do not print any resolved secret.
