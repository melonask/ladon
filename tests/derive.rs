use bitcoin::{
    NetworkKind,
    bip32::{Xpriv, Xpub},
};
use ladon::{Params, derive};

const MNEMONIC: &str =
    "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

#[test]
fn canonical_chains_derive_distinct_addresses() {
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
        assert_ne!(wallet.keys[0].address, wallet.keys[1].address);
    }
}

#[test]
fn legacy_chain_aliases_are_rejected() {
    assert!(
        derive(Params {
            chain: "ethereum".into(),
            mnemonic: Some(MNEMONIC.into()),
            ..Default::default()
        })
        .is_err()
    );
}

#[test]
fn bitcoin_extended_keys_honor_configured_network_and_paths() {
    let seed = bip39::Mnemonic::parse(MNEMONIC).unwrap().to_seed("");
    let master = Xpriv::new_master(NetworkKind::Main, &seed).unwrap();
    let secp = bitcoin::secp256k1::Secp256k1::new();
    let xpriv = master.to_string();
    let xpub = Xpub::from_priv(&secp, &master).to_string();
    let base = "m/44'/0'/0'/0";

    for (network, address_prefix) in [("bitcoin", "bc1"), ("testnet", "tb1")] {
        let from_xpriv = derive(Params {
            chain: "btc".into(),
            xpriv: Some(xpriv.clone()),
            xpriv_path: Some(base.into()),
            network: network.into(),
            index: Some(7),
            ..Default::default()
        })
        .unwrap();
        let from_xpub = derive(Params {
            chain: "btc".into(),
            xpub: Some(xpub.clone()),
            xpub_path: Some(base.into()),
            network: network.into(),
            index: Some(7),
            ..Default::default()
        })
        .unwrap();

        assert_eq!(from_xpriv.keys[0].path, "m/44'/0'/0'/0/7");
        assert_eq!(from_xpub.keys[0].path, "m/44'/0'/0'/0/7");
        assert!(from_xpriv.keys[0].address.starts_with(address_prefix));
        assert!(from_xpub.keys[0].address.starts_with(address_prefix));
    }
}
