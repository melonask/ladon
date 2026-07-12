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
