use ladon::{Params, WalletOutput, derive};
use std::process::Command;

const MNEMONIC: &str =
    "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

#[test]
fn programmatic_derive_creates_three_addresses_per_chain() {
    for chain in ["evm", "btc", "solana"] {
        let wallet = derive(Params {
            chain: chain.into(),
            mnemonic: Some(MNEMONIC.into()),
            num: 3,
            ..Default::default()
        })
        .unwrap();

        assert_eq!(wallet.chain, chain);
        assert_eq!(wallet.keys.len(), 3);
        assert_eq!(
            wallet
                .keys
                .iter()
                .map(|k| &k.address)
                .collect::<std::collections::HashSet<_>>()
                .len(),
            3
        );
        assert!(wallet.keys.iter().all(|k| !k.private_key.is_empty()));
    }
}

#[test]
fn cli_derive_creates_three_addresses_per_chain() {
    for chain in ["evm", "btc", "solana"] {
        let output = Command::new(env!("CARGO_BIN_EXE_ladon"))
            .args([
                "derive",
                "--chain",
                chain,
                "--mnemonic",
                MNEMONIC,
                "--num",
                "3",
            ])
            .output()
            .unwrap();

        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let wallet: WalletOutput = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(wallet.chain, chain);
        assert_eq!(wallet.keys.len(), 3);
        assert!(wallet.keys.iter().all(|k| !k.address.is_empty()));
    }
}

#[test]
fn cli_encrypt_and_decrypt_roundtrip() {
    let dir = std::env::temp_dir();
    let enc = dir.join(format!("ladon-{}-wallet.enc", std::process::id()));

    let derive_output = Command::new(env!("CARGO_BIN_EXE_ladon"))
        .args([
            "derive",
            "--chain",
            "evm",
            "--mnemonic",
            MNEMONIC,
            "--output",
            enc.to_str().unwrap(),
            "--encrypt",
            "--password",
            "test-password",
        ])
        .output()
        .unwrap();
    assert!(
        derive_output.status.success(),
        "{}",
        String::from_utf8_lossy(&derive_output.stderr)
    );

    let decrypt_output = Command::new(env!("CARGO_BIN_EXE_ladon"))
        .args([
            "decrypt",
            enc.to_str().unwrap(),
            "--password",
            "test-password",
        ])
        .output()
        .unwrap();

    let _ = std::fs::remove_file(enc);
    assert!(
        decrypt_output.status.success(),
        "{}",
        String::from_utf8_lossy(&decrypt_output.stderr)
    );
    let wallet: WalletOutput = serde_json::from_slice(&decrypt_output.stdout).unwrap();
    assert_eq!(wallet.keys.len(), 1);
}

#[test]
fn cli_writes_plain_output_file() {
    let path = std::env::temp_dir().join(format!("ladon-{}-wallet.json", std::process::id()));

    let output = Command::new(env!("CARGO_BIN_EXE_ladon"))
        .args([
            "derive",
            "--chain",
            "btc",
            "--mnemonic",
            MNEMONIC,
            "--num",
            "3",
            "--output",
            path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let data = std::fs::read(&path).unwrap();
    let _ = std::fs::remove_file(path);
    let wallet: WalletOutput = serde_json::from_slice(&data).unwrap();
    assert_eq!(wallet.chain, "btc");
    assert_eq!(wallet.keys.len(), 3);
}

#[test]
fn cli_rejects_unknown_options() {
    let output = Command::new(env!("CARGO_BIN_EXE_ladon"))
        .args(["derive", "--wat"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unexpected argument '--wat'"));
}
