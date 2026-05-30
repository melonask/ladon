use ladon::{KeyInfo, Params, derive};
use std::{fs, process::Command};

const MNEMONIC: &str =
    "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
const ANVIL_KEY: &str = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";

#[test]
#[ignore = "requires anvil and cast running at http://127.0.0.1:8545"]
fn anvil_transfers_to_generated_evm_addresses_and_between_them() {
    let keys = keys("evm");
    for key in &keys {
        run(
            "cast",
            &[
                "send",
                &key.address,
                "--value",
                "1ether",
                "--private-key",
                ANVIL_KEY,
                "--rpc-url",
                "http://127.0.0.1:8545",
            ],
        );
    }
    run(
        "cast",
        &[
            "send",
            &keys[1].address,
            "--value",
            "0.1ether",
            "--private-key",
            &keys[0].private_key,
            "--rpc-url",
            "http://127.0.0.1:8545",
        ],
    );
}

#[test]
#[ignore = "requires solana-test-validator and solana CLI configured for localhost"]
fn solana_transfers_to_generated_addresses_and_between_them() {
    let keys = keys("solana");
    let payer =
        std::env::temp_dir().join(format!("ladon-solana-payer-{}.json", std::process::id()));
    run(
        "solana-keygen",
        &[
            "new",
            "--no-bip39-passphrase",
            "--silent",
            "--force",
            "--outfile",
            payer.to_str().unwrap(),
        ],
    );
    let payer_address = read("solana-keygen", &["pubkey", payer.to_str().unwrap()]);
    run(
        "solana",
        &["airdrop", "10", payer_address.trim(), "--url", "localhost"],
    );

    for key in &keys {
        run(
            "solana",
            &[
                "transfer",
                &key.address,
                "1",
                "--keypair",
                payer.to_str().unwrap(),
                "--allow-unfunded-recipient",
                "--url",
                "localhost",
            ],
        );
    }

    let from = solana_keypair_file(&keys[0]);
    run(
        "solana",
        &[
            "transfer",
            &keys[1].address,
            "0.1",
            "--keypair",
            from.to_str().unwrap(),
            "--url",
            "localhost",
        ],
    );
    let _ = fs::remove_file(from);
    let _ = fs::remove_file(payer);
}

#[test]
#[ignore = "requires bitcoind regtest and bitcoin-cli"]
fn bitcoin_regtest_transfers_to_generated_addresses_and_between_them() {
    let keys = keys("btc");
    let datadir = std::env::var("LADON_BITCOIN_DATADIR").unwrap_or_else(|_| {
        std::env::temp_dir()
            .join("ladon-bitcoin-regtest")
            .display()
            .to_string()
    });
    let wallet = format!("ladon-test-{}", std::process::id());
    let wallet_arg = format!("-rpcwallet={wallet}");
    let _ = Command::new("bitcoin-cli")
        .args([
            "-regtest",
            &format!("-datadir={datadir}"),
            "createwallet",
            &wallet,
        ])
        .output();
    let miner = bitcoin_read(&datadir, &[&wallet_arg, "getnewaddress"]);
    bitcoin_run(&datadir, &["generatetoaddress", "101", miner.trim()]);

    for key in &keys {
        bitcoin_run(&datadir, &[&wallet_arg, "sendtoaddress", &key.address, "1"]);
    }
    let desc_info = bitcoin_read(
        &datadir,
        &[
            "getdescriptorinfo",
            &format!("wpkh({})", keys[0].wif.as_ref().unwrap()),
        ],
    );
    let desc = serde_json::from_str::<serde_json::Value>(&desc_info).unwrap()["descriptor"]
        .as_str()
        .unwrap()
        .to_string();
    let import = format!(r#"[{{"desc":"{desc}","timestamp":"now"}}]"#);
    bitcoin_run(&datadir, &[&wallet_arg, "importdescriptors", &import]);
    bitcoin_run(&datadir, &["generatetoaddress", "1", miner.trim()]);
    bitcoin_run(
        &datadir,
        &[&wallet_arg, "sendtoaddress", &keys[1].address, "0.1"],
    );
}

fn keys(chain: &str) -> Vec<KeyInfo> {
    derive(Params {
        chain: chain.into(),
        mnemonic: Some(MNEMONIC.into()),
        num: 3,
        network: if chain == "btc" {
            "regtest".into()
        } else {
            String::new()
        },
        ..Default::default()
    })
    .unwrap()
    .keys
}

fn solana_keypair_file(key: &KeyInfo) -> std::path::PathBuf {
    let mut bytes = hex::decode(&key.private_key).unwrap();
    bytes.extend(hex::decode(&key.public_key).unwrap());
    let json = format!(
        "[{}]",
        bytes
            .iter()
            .map(u8::to_string)
            .collect::<Vec<_>>()
            .join(",")
    );
    let path = std::env::temp_dir().join(format!("ladon-solana-{}.json", std::process::id()));
    fs::write(&path, json).unwrap();
    path
}

fn run(bin: &str, args: &[&str]) {
    let output = Command::new(bin).args(args).output().unwrap();
    assert!(
        output.status.success(),
        "{} {}\nstdout: {}\nstderr: {}",
        bin,
        args.join(" "),
        out(&output.stdout),
        out(&output.stderr)
    );
}

fn read(bin: &str, args: &[&str]) -> String {
    let output = Command::new(bin).args(args).output().unwrap();
    assert!(
        output.status.success(),
        "{} {}\nstdout: {}\nstderr: {}",
        bin,
        args.join(" "),
        out(&output.stdout),
        out(&output.stderr)
    );
    out(&output.stdout)
}

fn bitcoin_run(datadir: &str, args: &[&str]) {
    let datadir_arg = format!("-datadir={datadir}");
    let mut all = vec!["-regtest", datadir_arg.as_str()];
    all.extend_from_slice(args);
    run("bitcoin-cli", &all);
}

fn bitcoin_read(datadir: &str, args: &[&str]) -> String {
    let datadir_arg = format!("-datadir={datadir}");
    let mut all = vec!["-regtest", datadir_arg.as_str()];
    all.extend_from_slice(args);
    read("bitcoin-cli", &all)
}

fn out(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}
