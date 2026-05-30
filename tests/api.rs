use ladon::{
    EncryptedWallet, HARDENED, Params, child_path, decrypt_data, default_path, derive, derive_btc,
    derive_evm, derive_slip10, derive_solana, encrypt_data, mnemonic_for, parse_indexes,
    parse_path, secp_key,
};

const MNEMONIC: &str =
    "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

#[test]
fn path_and_index_helpers_are_strict() {
    assert_eq!(default_path("evm", 1, 2, false), "m/44'/60'/1'/2/0");
    assert_eq!(default_path("btc", 1, 2, false), "m/44'/0'/1'/2/0");
    assert_eq!(default_path("solana", 1, 2, false), "m/44'/501'/1'/2'");
    assert_eq!(child_path("m/44'/60'/0'/0", 7, "evm"), "m/44'/60'/0'/7");
    assert_eq!(
        child_path("m/44'/501'/0'/0'", 7, "solana"),
        "m/44'/501'/0'/0'/7'"
    );
    assert_eq!(parse_indexes("0, 2,9").unwrap(), vec![0, 2, 9]);
    assert!(parse_indexes("0,nope").is_err());
    assert_eq!(
        parse_path("m/44'/60'/0'/7").unwrap(),
        vec![44 + HARDENED, 60 + HARDENED, HARDENED, 7]
    );
    assert!(parse_path(&format!("{}'", HARDENED)).is_err());
}

#[test]
fn mnemonic_and_encryption_helpers_work_and_reject_bad_input() {
    let parsed = mnemonic_for(Some(MNEMONIC.into()), 12).unwrap();
    assert_eq!(parsed.to_string(), MNEMONIC);
    assert_eq!(mnemonic_for(None, 12).unwrap().word_count(), 12);
    assert_eq!(mnemonic_for(None, 24).unwrap().word_count(), 24);
    assert!(mnemonic_for(Some("not a mnemonic".into()), 12).is_err());

    let encrypted = encrypt_data("secret payload", "correct horse").unwrap();
    let envelope: EncryptedWallet = serde_json::from_str(&encrypted).unwrap();
    assert_eq!(
        decrypt_data(&envelope, "correct horse").unwrap(),
        "secret payload"
    );
    assert!(decrypt_data(&envelope, "wrong horse").is_err());
}

#[test]
fn explicit_generators_match_top_level_derivation() {
    let mnemonic = mnemonic_for(Some(MNEMONIC.into()), 12).unwrap();
    let seed = mnemonic.to_seed("");

    let evm = derive(Params {
        chain: "evm".into(),
        mnemonic: Some(MNEMONIC.into()),
        num: 1,
        ..Default::default()
    })
    .unwrap();
    let btc = derive(Params {
        chain: "btc".into(),
        mnemonic: Some(MNEMONIC.into()),
        num: 1,
        ..Default::default()
    })
    .unwrap();
    let sol = derive(Params {
        chain: "solana".into(),
        mnemonic: Some(MNEMONIC.into()),
        num: 1,
        ..Default::default()
    })
    .unwrap();

    assert_eq!(
        derive_evm(&seed, &evm.keys[0].path, 0).unwrap().address,
        evm.keys[0].address
    );
    assert_eq!(
        derive_btc(&seed, &btc.keys[0].path, 0, bitcoin::Network::Bitcoin)
            .unwrap()
            .address,
        btc.keys[0].address
    );
    assert_eq!(
        derive_solana(&seed, &sol.keys[0].path, 0, "full", "")
            .unwrap()
            .address,
        sol.keys[0].address
    );
}

#[test]
fn xpub_xpriv_indexes_and_modes_are_usable() {
    let base = derive(Params {
        chain: "evm".into(),
        mnemonic: Some(MNEMONIC.into()),
        num: 3,
        ..Default::default()
    })
    .unwrap();

    let from_xpub = derive(Params {
        chain: "evm".into(),
        xpub: base.master_xpub.clone(),
        num: 3,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(
        from_xpub
            .keys
            .iter()
            .map(|k| &k.address)
            .collect::<Vec<_>>(),
        base.keys.iter().map(|k| &k.address).collect::<Vec<_>>()
    );
    assert!(from_xpub.keys.iter().all(|k| k.private_key == "WATCH-ONLY"));

    let from_xpriv = derive(Params {
        chain: "evm".into(),
        xpriv: base.keys[0].xprv.clone(),
        xpriv_path: Some(base.keys[0].path.clone()),
        index: Some(1),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(from_xpriv.keys.len(), 1);
    assert!(from_xpriv.keys[0].private_key.starts_with("0x"));

    let sparse = derive(Params {
        chain: "evm".into(),
        mnemonic: Some(MNEMONIC.into()),
        indexes: Some("0,2".into()),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(
        sparse.keys.iter().map(|k| k.index).collect::<Vec<_>>(),
        vec![0, 2]
    );
}

#[test]
fn solana_modes_and_low_level_derivation_are_validated() {
    let cold = derive(Params {
        chain: "solana".into(),
        mnemonic: Some(MNEMONIC.into()),
        solana_mode: "cold-export".into(),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(cold.keys[0].private_key, "HIDDEN");

    let pda = derive(Params {
        chain: "solana".into(),
        mnemonic: Some(MNEMONIC.into()),
        solana_mode: "pda".into(),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(pda.keys[0].private_key, "PDA_RECEIVE_ONLY");
    assert_ne!(pda.keys[0].address, cold.keys[0].address);

    let mnemonic = mnemonic_for(Some(MNEMONIC.into()), 12).unwrap();
    assert!(derive_slip10(&mnemonic.to_seed(""), &[0]).is_err());
}

#[test]
fn secp_key_rejects_invalid_paths() {
    let mnemonic = mnemonic_for(Some(MNEMONIC.into()), 12).unwrap();
    assert!(
        secp_key(
            &mnemonic.to_seed(""),
            "m/44'/60'/0'/0",
            bitcoin::NetworkKind::Main
        )
        .is_ok()
    );
    assert!(
        secp_key(
            &mnemonic.to_seed(""),
            "not/a/path",
            bitcoin::NetworkKind::Main
        )
        .is_err()
    );
}
