# Getting started

Install Ladon, copy `Config.toml`, and supply the referenced mnemonic without placing it in the file:

```sh
cargo install ladon
export LADON_MNEMONIC='your BIP-39 mnemonic'
ladon --config Config.toml check
```

`check` validates configuration and secret *references*. It never prints mnemonic, passphrase, xpriv, or database credentials.

Use the Rust library for application derivation:

```rust
use ladon::{derive, Params};
let wallet = derive(Params { chain: "evm".into(), mnemonic: Some("abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about".into()), num: 1, ..Default::default() })?;
println!("{}", wallet.keys[0].address);
# Ok::<(), anyhow::Error>(())
```

Keep returned private material out of logs and persistent application output.
