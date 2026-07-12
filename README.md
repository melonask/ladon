# Ladon

Ladon is a configuration-driven address-pool service for EVM, Bitcoin, and Solana. It validates a universal TOML configuration and pre-derives addresses into SQLite or PostgreSQL for another service to consume.

**Documentation: https://melonask.github.io/ladon/**

## Run

```sh
export LADON_MNEMONIC='your BIP-39 mnemonic'
ladon --config /etc/stack/Config.toml check
ladon --config /etc/stack/Config.toml pool
```

The public CLI intentionally provides only `check` and `pool`. `check` validates configuration and secret references without printing secret values. `pool` maintains available rows for configured chains.

The supplied [Config.toml](Config.toml) is a complete universal-schema example. Keep mnemonics, xprivs, passphrases, and PostgreSQL URLs in environment variables or protected secret files, never in configuration or logs.

## Development

```sh
cargo fmt --check
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
```

## License

MIT
