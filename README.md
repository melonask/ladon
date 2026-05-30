# ladon

<img align="right" src="https://raw.githubusercontent.com/melonask/ladon/refs/heads/main/logo.svg" alt="Fast, minimal, multi-chain HD wallet CLI and library — EVM, Bitcoin, Solana" width="120" />

> *Like the hundred-headed serpent of Greek myth, Ladon generates as many addresses as you need.*

Fast, minimal, multi-chain HD wallet CLI and library — EVM, Bitcoin, Solana.

---

## Install

```sh
cargo install ladon
```

---

## CLI

### Derive

```sh
# EVM — 5 addresses from a fresh mnemonic
ladon derive --chain evm --num 5

# Bitcoin — import existing mnemonic
ladon derive --chain btc --mnemonic "word1 word2 ... word12"

# Solana — cold-export (addresses only, private keys hidden)
ladon derive --chain solana --solana-mode cold-export --num 10

# Solana — PDA mode
ladon derive --chain solana --solana-mode pda --program-id <BASE58_PROGRAM_ID>

# Specific indexes (overrides --index / --num)
ladon derive --chain evm --indexes 0,5,99

# From xpub (watch-only)
ladon derive --chain evm --xpub xpub6C...

# From xpriv
ladon derive --chain evm --xpriv xprv9s...

# Save to file (JSON)
ladon derive --chain evm --num 20 --output wallet.json

# Encrypt output
ladon derive --chain evm --output wallet.enc --encrypt --password "mypassword"
```

#### Key flags

| Flag | Default | Description |
|------|---------|-------------|
| `--chain` | `evm` | Target chain: `evm`, `btc`, `solana` |
| `--num` | `1` | Number of addresses |
| `--index` | — | Single specific index |
| `--indexes` | — | Comma-separated indexes, e.g. `0,3,7` |
| `--account` | `0` | BIP44 account |
| `--change` | `0` | BIP44 change |
| `--network` | `bitcoin` | Bitcoin network: `bitcoin`, `testnet`, `signet`, `regtest` |
| `--passphrase` | `""` | BIP39 passphrase |
| `--strength` | `12` | Mnemonic word count (`12` or `24`) |
| `--solana-mode` | `full` | `full`, `cold-export`, `hsm-sim`, `pda` |
| `--program-id` | — | Base58 program ID for PDA mode |
| `--xpub` | — | Derive watch-only from xpub |
| `--xpriv` | — | Derive from xpriv |
| `--encrypt` | false | Encrypt output with a password |
| `--password` | — | Encryption password |
| `--output` | — | Save JSON/encrypted wallet to file |

### Decrypt

```sh
ladon decrypt wallet.enc --password "mypassword"
ladon decrypt wallet.enc --password "mypassword" --output wallet.json
```

---

## Library

```rust
use ladon::{derive, decrypt_data, encrypt_data, Params, WalletOutput};

// Generate EVM addresses
let params = Params {
    chain: "evm".into(),
    num: 5,
    ..Default::default()
};
let wallet: WalletOutput = derive(params)?;

for key in &wallet.keys {
    println!("{}: {}", key.index, key.address);
}

// Encrypt / decrypt
let json = serde_json::to_string(&wallet)?;
let encrypted = encrypt_data(&json, "my-password")?;
let decrypted = decrypt_data(&serde_json::from_str(&encrypted)?, "my-password")?;
```

### `Params` fields

All fields have sensible defaults via `Default`. Only set what you need.

```rust
pub struct Params {
    pub chain: String,           // "evm" | "btc" | "solana"
    pub mnemonic: Option<String>,
    pub passphrase: String,
    pub index: Option<u32>,
    pub indexes: Option<String>, // comma-separated, e.g. "0,5,99"
    pub account: u32,
    pub change: u32,
    pub num: u32,
    pub network: String,         // Bitcoin only: "bitcoin" | "testnet" | "signet" | "regtest"
    pub strength: u32,           // 12 or 24
    pub hw_sim: bool,
    pub xpub: Option<String>,
    pub xpub_path: Option<String>,
    pub xpriv: Option<String>,
    pub xpriv_path: Option<String>,
    pub solana_mode: String,     // "full" | "cold-export" | "hsm-sim" | "pda"
    pub program_id: String,
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
- Use `--solana-mode cold-export` or `--encrypt` when storing output.
- `--password` on the CLI exposes the password in shell history; prefer environment-specific secret handling for automation.
- Ed25519 derivation follows **SLIP-0010** (all segments hardened).

---

## License

MIT
