use anyhow::{Context, Result};
use bip39::{Language, Mnemonic};
use bitcoin::{
    Network, NetworkKind,
    bip32::{ChainCode, ChildNumber, DerivationPath, Fingerprint, Xpriv, Xpub},
};
use ed25519_dalek::SigningKey;
use serde::{Deserialize, Serialize};
use solana_pubkey::Pubkey;
use std::str::FromStr;
use zeroize::Zeroize;

// ── Constants ────────────────────────────────────────────────────────────────

pub const HARDENED: u32 = 0x8000_0000;

// ── Public types ─────────────────────────────────────────────────────────────

/// Derivation parameters for [`derive`]. All fields have sensible defaults.
#[derive(Clone, Debug, Default)]
pub struct Params {
    pub chain: String,
    pub mnemonic: Option<String>,
    pub passphrase: String,
    pub index: Option<u32>,
    /// Comma-separated indexes / inclusive ranges, e.g. `"0,5,22-44"`.
    /// Supersedes `index` and `num`.
    pub indexes: Option<String>,
    pub account: u32,
    pub change: u32,
    pub num: u32,
    /// Bitcoin address network: `bitcoin`, `testnet`, `signet`, or `regtest`.
    pub network: String,
    /// Mnemonic word count for generation: `12` or `24`.
    pub strength: u32,
    pub hw_sim: bool,
    pub xpub: Option<String>,
    pub xpub_path: Option<String>,
    pub xpriv: Option<String>,
    pub xpriv_path: Option<String>,
    /// `"full"` | `"cold-export"` | `"hsm-sim"` | `"pda"`
    pub solana_mode: String,
    pub program_id: String,
}

/// A single derived key / address tuple.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct KeyInfo {
    pub index: u32,
    pub path: String,
    pub xprv: Option<String>,
    pub xpub: Option<String>,
    pub private_key: String,
    pub public_key: String,
    pub address: String,
    pub wif: Option<String>,
}

impl Drop for KeyInfo {
    fn drop(&mut self) {
        self.private_key.zeroize();
        if let Some(ref mut x) = self.xprv {
            x.zeroize();
        }
        if let Some(ref mut w) = self.wif {
            w.zeroize();
        }
    }
}

/// Full wallet output returned by [`derive`] and all lower-level generators.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WalletOutput {
    pub mnemonic: String,
    pub passphrase: String,
    pub chain: String,
    pub master_xprv: Option<String>,
    pub master_xpub: Option<String>,
    pub keys: Vec<KeyInfo>,
}

/// On-disk envelope produced by [`encrypt_data`], consumed by [`decrypt_data`].
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EncryptedWallet {
    pub version: u32,
    pub salt: String,
    pub nonce: String,
    pub ciphertext: String,
}

// ── Top-level API ─────────────────────────────────────────────────────────────

/// Generate one or more keys / addresses. Single entry-point for programmatic use.
pub fn derive(p: Params) -> Result<WalletOutput> {
    let chain = {
        let s = p.chain.to_lowercase();
        if s.is_empty() { "evm".to_string() } else { s }
    };

    let num = if p.num == 0 { 1 } else { p.num };
    let base = default_path(&chain, p.account, p.change, p.hw_sim);

    if let Some(xpriv_str) = p.xpriv {
        let base = p
            .xpriv_path
            .unwrap_or_else(|| base.trim_end_matches("/0").to_string());
        return derive_from_xpriv(
            &xpriv_str,
            &base,
            p.index,
            num,
            &chain,
            &p.solana_mode,
            &p.program_id,
            &p.indexes,
        );
    }

    if let Some(xpub_str) = p.xpub {
        let base = p
            .xpub_path
            .unwrap_or_else(|| base.trim_end_matches("/0").to_string());
        return derive_from_xpub(&xpub_str, &base, p.index, num, &chain);
    }

    let mnemonic = mnemonic_for(p.mnemonic, p.strength)?;
    let seed = mnemonic.to_seed(&p.passphrase);
    derive_from_seed(
        &seed,
        &base,
        p.index,
        num,
        &mnemonic,
        &p.passphrase,
        &chain,
        &p.solana_mode,
        &p.program_id,
        &p.network,
        &p.indexes,
    )
}

// ── Path helpers ──────────────────────────────────────────────────────────────

/// Return the default BIP-44 derivation base path for `chain`.
pub fn default_path(chain: &str, account: u32, change: u32, _hw_sim: bool) -> String {
    match chain {
        "evm" | "ethereum" => format!("m/44'/60'/{account}'/{change}/0"),
        "btc" | "bitcoin" => format!("m/44'/0'/{account}'/{change}/0"),
        "solana" => format!("m/44'/501'/{account}'/{change}'"),
        _ => format!("m/44'/60'/{account}'/{change}/0"),
    }
}

/// Build a child derivation path from `base` and `index` for `chain`.
pub fn child_path(base: &str, index: u32, chain: &str) -> String {
    if is_ed25519(chain) {
        let normalised: String = base
            .split('/')
            .map(|seg| {
                if seg == "m" || seg.is_empty() || seg.ends_with('\'') {
                    seg.to_string()
                } else {
                    format!("{seg}'")
                }
            })
            .collect::<Vec<_>>()
            .join("/");
        format!("{normalised}/{index}'")
    } else {
        let base = base.trim_end_matches(|c: char| c.is_ascii_digit() || c == '/');
        format!("{base}/{index}")
    }
}

/// Parse a derivation-path string into a sequence of BIP-32 child indexes.
pub fn parse_path(path: &str) -> Result<Vec<u32>> {
    let trimmed = path
        .strip_prefix("m/")
        .or_else(|| path.strip_prefix('m'))
        .unwrap_or(path);

    if trimmed.is_empty() {
        return Ok(vec![]);
    }

    trimmed
        .split('/')
        .filter(|s| !s.is_empty())
        .map(|part| {
            if let Some(num_str) = part.strip_suffix('\'') {
                let n: u32 = num_str.parse().context("Invalid hardened path segment")?;
                if n >= HARDENED {
                    anyhow::bail!("Hardened index {n} out of range (max {})", HARDENED - 1);
                }
                Ok(n + HARDENED)
            } else {
                let n: u32 = part.parse().context("Invalid path segment")?;
                if n >= HARDENED {
                    anyhow::bail!("Standard index {n} out of range (max {})", HARDENED - 1);
                }
                Ok(n)
            }
        })
        .collect()
}

/// Parse a comma-separated index / range list into a `Vec<u32>`.
pub fn parse_indexes(s: &str) -> Result<Vec<u32>> {
    s.split(',')
        .flat_map(|raw| {
            let token = raw.trim();
            if token.is_empty() {
                return vec![Err(anyhow::anyhow!("Empty index token"))];
            }

            if let Some((start, end)) = token.split_once('-') {
                let start = match start.trim().parse::<u32>() {
                    Ok(v) => v,
                    Err(_) => {
                        return vec![Err(anyhow::anyhow!(
                            "Invalid range start: '{}'",
                            start.trim()
                        ))];
                    }
                };
                let end = match end.trim().parse::<u32>() {
                    Ok(v) => v,
                    Err(_) => {
                        return vec![Err(anyhow::anyhow!("Invalid range end: '{}'", end.trim()))];
                    }
                };
                if start > end {
                    return vec![Err(anyhow::anyhow!("Descending range: '{token}'"))];
                }
                return (start..=end).map(Ok).collect();
            }

            vec![
                token
                    .parse::<u32>()
                    .with_context(|| format!("Invalid index: '{token}'")),
            ]
        })
        .collect()
}

// ── Seed-based generation ─────────────────────────────────────────────────────

/// Parse or generate a mnemonic.
pub fn mnemonic_for(raw: Option<String>, strength: u32) -> Result<Mnemonic> {
    match raw {
        Some(s) => {
            let sanitised = s
                .to_lowercase()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            Mnemonic::parse_in(Language::English, sanitised).context("Invalid mnemonic")
        }
        None => {
            let words = if strength == 24 { 24 } else { 12 };
            Mnemonic::generate_in(Language::English, words).context("Failed to generate mnemonic")
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn derive_from_seed(
    seed: &[u8],
    base_path: &str,
    specific_index: Option<u32>,
    num: u32,
    mnemonic: &Mnemonic,
    passphrase: &str,
    chain: &str,
    solana_mode: &str,
    program_id: &str,
    network: &str,
    indexes: &Option<String>,
) -> Result<WalletOutput> {
    let btc_network = bitcoin_network(network);
    let indices = resolve_indices(indexes, specific_index, num)?;
    let keys = indices
        .iter()
        .map(|&idx| {
            let path = child_path(base_path, idx, chain);
            match chain {
                "evm" | "ethereum" => derive_evm(seed, &path, idx),
                "btc" | "bitcoin" => derive_btc(seed, &path, idx, btc_network),
                "solana" => derive_solana(seed, &path, idx, solana_mode, program_id),
                other => anyhow::bail!("Unsupported chain '{other}'. Use: evm, btc, solana"),
            }
        })
        .collect::<Result<Vec<_>>>()?;

    let mut wallet = WalletOutput {
        mnemonic: mnemonic.to_string(),
        passphrase: passphrase.to_string(),
        chain: chain.to_string(),
        master_xprv: Some("hidden".to_string()),
        master_xpub: None,
        keys,
    };

    let account_base = base_path.trim_end_matches(|c: char| c.is_ascii_digit() || c == '/');
    if is_ed25519(chain) {
        if let Ok(idxs) = parse_path(account_base)
            && let Ok(derived) = derive_slip10(seed, &idxs)
        {
            let sk: [u8; 32] = derived[..32].try_into().unwrap();
            let pub_bytes = SigningKey::from_bytes(&sk).verifying_key().to_bytes();
            let mut xpub = derived[32..].to_vec();
            xpub.extend_from_slice(&pub_bytes);
            wallet.master_xpub = Some(hex::encode(xpub));
        }
    } else if let Ok(key) = secp_key(seed, account_base, network_kind(btc_network)) {
        let secp = bitcoin::secp256k1::Secp256k1::new();
        wallet.master_xpub = Some(Xpub::from_priv(&secp, &key).to_string());
    }

    Ok(wallet)
}

// ── xpub / xpriv derivation ───────────────────────────────────────────────────

pub fn derive_from_xpub(
    xpub_str: &str,
    base: &str,
    specific_index: Option<u32>,
    num: u32,
    chain: &str,
) -> Result<WalletOutput> {
    if is_ed25519(chain) {
        anyhow::bail!("xpub mode is not supported for {chain} (Ed25519 curve)");
    }

    let xpub = parse_xpub(xpub_str)?;
    let secp = bitcoin::secp256k1::Secp256k1::new();
    let indices = resolve_indices(&None, specific_index, num)?;

    let keys = indices
        .iter()
        .map(|&idx| {
            let path = format!("{}/{idx}", base.trim_end_matches('/'));
            let child = xpub.ckd_pub(&secp, ChildNumber::from_normal_idx(idx)?)?;
            let pk_bytes = child.public_key.serialize_uncompressed();
            let address = match chain {
                "evm" | "ethereum" => eth_address(&pk_bytes),
                "btc" | "bitcoin" => {
                    let cpk =
                        bitcoin::CompressedPublicKey::from_slice(&child.public_key.serialize())
                            .expect("valid compressed pk");
                    bitcoin::Address::p2wpkh(&cpk, bitcoin::Network::Bitcoin).to_string()
                }
                other => anyhow::bail!("xpub mode not supported for '{other}'"),
            };
            Ok(KeyInfo {
                index: idx,
                path,
                xprv: None,
                xpub: Some(xpub_str.to_string()),
                private_key: "WATCH-ONLY".to_string(),
                public_key: hex::encode(pk_bytes),
                address,
                wif: None,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(WalletOutput {
        mnemonic: "WATCH-ONLY".to_string(),
        passphrase: String::new(),
        chain: chain.to_string(),
        master_xprv: None,
        master_xpub: Some(xpub_str.to_string()),
        keys,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn derive_from_xpriv(
    xpriv_str: &str,
    base: &str,
    specific_index: Option<u32>,
    num: u32,
    chain: &str,
    solana_mode: &str,
    program_id: &str,
    indexes: &Option<String>,
) -> Result<WalletOutput> {
    let indices = resolve_indices(indexes, specific_index, num)?;

    let keys = if is_ed25519(chain) {
        let raw = hex::decode(xpriv_str.strip_prefix("0x").unwrap_or(xpriv_str))
            .context("Invalid hex for Ed25519 xpriv")?;
        let parent: [u8; 64] = raw
            .try_into()
            .map_err(|_| anyhow::anyhow!("Ed25519 xpriv must be 64 bytes (key + chain code)"))?;

        indices
            .iter()
            .map(|&idx| {
                let path = format!("{}/{idx}'", base.trim_end_matches('/'));
                let child = slip10_child(&parent, idx + HARDENED)?;
                let sk: [u8; 32] = child[..32].try_into().unwrap();
                let chain_code = &child[32..];
                let signing = SigningKey::from_bytes(&sk);
                let pubkey = Pubkey::new_from_array(signing.verifying_key().to_bytes());
                let (address, private_key, xprv, xpub) =
                    solana_key_info(idx, &pubkey, sk, chain_code, solana_mode, program_id)?;
                Ok(KeyInfo {
                    index: idx,
                    path,
                    xprv,
                    xpub,
                    private_key,
                    public_key: hex::encode(signing.verifying_key().to_bytes()),
                    address,
                    wif: None,
                })
            })
            .collect::<Result<Vec<_>>>()?
    } else {
        let secp = bitcoin::secp256k1::Secp256k1::new();
        let parent = Xpriv::from_str(xpriv_str).context("Invalid BIP32 xpriv")?;

        indices
            .iter()
            .map(|&idx| {
                let path = format!("{}/{idx}", base.trim_end_matches('/'));
                let child = parent.derive_priv(&secp, &[ChildNumber::from_normal_idx(idx)?])?;
                let mut priv_key = child.to_priv();
                priv_key.compressed = true;
                let pub_key = priv_key.public_key(&secp);
                let pub_uncompressed = pub_key.inner.serialize_uncompressed();
                let pub_compressed = pub_key.inner.serialize();
                let sk_bytes = child.private_key.secret_bytes();

                let (address, private_key, wif) = match chain {
                    "evm" | "ethereum" => (
                        eth_address(&pub_uncompressed),
                        format!("0x{}", hex::encode(sk_bytes)),
                        None,
                    ),
                    "btc" | "bitcoin" => {
                        let cpk = bitcoin::CompressedPublicKey::from_slice(&pub_compressed)
                            .expect("valid pk");
                        let addr =
                            bitcoin::Address::p2wpkh(&cpk, bitcoin::Network::Bitcoin).to_string();
                        let wif = priv_key.to_wif();
                        (addr, wif.clone(), Some(wif))
                    }
                    other => anyhow::bail!("Chain '{other}' not supported for xpriv mode"),
                };

                Ok(KeyInfo {
                    index: idx,
                    path,
                    xprv: Some(child.to_string()),
                    xpub: Some(Xpub::from_priv(&secp, &child).to_string()),
                    private_key,
                    public_key: hex::encode(pub_compressed),
                    address,
                    wif,
                })
            })
            .collect::<Result<Vec<_>>>()?
    };

    Ok(WalletOutput {
        mnemonic: "derived from xpriv".to_string(),
        passphrase: String::new(),
        chain: chain.to_string(),
        master_xprv: Some(xpriv_str.to_string()),
        master_xpub: None,
        keys,
    })
}

// ── Per-chain key generators ──────────────────────────────────────────────────

pub fn derive_evm(seed: &[u8], path: &str, idx: u32) -> Result<KeyInfo> {
    let secp = bitcoin::secp256k1::Secp256k1::new();
    let child = secp_key(seed, path, NetworkKind::Main)?;
    let pub_key = child.to_priv().public_key(&secp);
    let pub_uncompressed = pub_key.inner.serialize_uncompressed();
    let sk_bytes = child.private_key.secret_bytes();

    Ok(KeyInfo {
        index: idx,
        path: path.to_string(),
        xprv: Some(child.to_string()),
        xpub: Some(Xpub::from_priv(&secp, &child).to_string()),
        private_key: format!("0x{}", hex::encode(sk_bytes)),
        public_key: format!("0x{}", hex::encode(pub_uncompressed)),
        address: eth_address(&pub_uncompressed),
        wif: None,
    })
}

pub fn derive_btc(seed: &[u8], path: &str, idx: u32, network: Network) -> Result<KeyInfo> {
    let secp = bitcoin::secp256k1::Secp256k1::new();
    let child = secp_key(seed, path, network_kind(network))?;
    let mut priv_key = child.to_priv();
    priv_key.compressed = true;
    let pub_key = priv_key.public_key(&secp);
    let cpk =
        bitcoin::CompressedPublicKey::from_slice(&pub_key.inner.serialize()).expect("valid pk");
    let address = bitcoin::Address::p2wpkh(&cpk, network).to_string();
    let wif = priv_key.to_wif();

    Ok(KeyInfo {
        index: idx,
        path: path.to_string(),
        xprv: Some(child.to_string()),
        xpub: Some(Xpub::from_priv(&secp, &child).to_string()),
        private_key: wif.clone(),
        public_key: hex::encode(pub_key.inner.serialize()),
        address,
        wif: Some(wif),
    })
}

pub fn derive_solana(
    seed: &[u8],
    path: &str,
    idx: u32,
    mode: &str,
    program_id: &str,
) -> Result<KeyInfo> {
    let path_idxs = parse_path(path)?;
    let derived = derive_slip10(seed, &path_idxs)?;
    let sk: [u8; 32] = derived[..32].try_into().unwrap();
    let signing = SigningKey::from_bytes(&sk);
    let pubkey = Pubkey::new_from_array(signing.verifying_key().to_bytes());
    let (address, private_key, xprv, xpub) =
        solana_key_info(idx, &pubkey, sk, &derived[32..], mode, program_id)?;

    Ok(KeyInfo {
        index: idx,
        path: path.to_string(),
        xprv,
        xpub,
        private_key,
        public_key: hex::encode(signing.verifying_key().to_bytes()),
        address,
        wif: None,
    })
}

// ── Cryptographic primitives ──────────────────────────────────────────────────

/// Derive a secp256k1 extended private key at `path` from `seed`.
pub fn secp_key(seed: &[u8], path: &str, network: NetworkKind) -> Result<Xpriv> {
    let secp = bitcoin::secp256k1::Secp256k1::new();
    let master = Xpriv::new_master(network, seed).context("Master key creation failed")?;
    let stripped = path.strip_prefix("m/").unwrap_or(path);
    let dp = DerivationPath::from_str(stripped).context("Invalid derivation path")?;
    master
        .derive_priv(&secp, &dp)
        .context("Key derivation failed")
}

/// SLIP-0010 Ed25519 derivation from `seed` over a hardened `path`.
pub fn derive_slip10(seed: &[u8], path: &[u32]) -> Result<[u8; 64]> {
    use hmac::{KeyInit, Mac};

    let mut i: [u8; 64] = {
        let mut mac = <hmac::Hmac<sha2::Sha512>>::new_from_slice(b"ed25519 seed")
            .expect("HMAC accepts any key length");
        mac.update(seed);
        mac.finalize().into_bytes().into()
    };

    for &idx in path {
        if idx < HARDENED {
            anyhow::bail!(
                "SLIP-0010: path index {idx:#x} is unhardened — all Ed25519 segments must be hardened"
            );
        }
        let mut mac = <hmac::Hmac<sha2::Sha512>>::new_from_slice(&i[32..])
            .expect("HMAC accepts any key length");
        mac.update(&[0u8]);
        mac.update(&i[..32]);
        mac.update(&idx.to_be_bytes());
        i = mac.finalize().into_bytes().into();
    }

    Ok(i)
}

/// Derive a single hardened SLIP-0010 child from `parent` (key + chain code).
pub fn slip10_child(parent: &[u8; 64], child_index: u32) -> Result<[u8; 64]> {
    use hmac::{KeyInit, Mac};

    if child_index < HARDENED {
        anyhow::bail!("SLIP-0010: child index {child_index:#x} must be hardened");
    }

    let mut mac = <hmac::Hmac<sha2::Sha512>>::new_from_slice(&parent[32..])
        .context("HMAC init from parent chain code")?;
    mac.update(&[0u8]);
    mac.update(&parent[..32]);
    mac.update(&child_index.to_be_bytes());

    Ok(mac.finalize().into_bytes().into())
}

/// Compute an EIP-55 checksummed Ethereum address from an uncompressed public key.
pub fn eth_address(pubkey_uncompressed: &[u8]) -> String {
    use tiny_keccak::{Hasher, Keccak};

    let mut hash = [0u8; 32];
    let mut k = Keccak::v256();
    k.update(&pubkey_uncompressed[1..]);
    k.finalize(&mut hash);

    let addr_hex = hex::encode(&hash[12..]);

    let mut cs_hash = [0u8; 32];
    let mut k2 = Keccak::v256();
    k2.update(addr_hex.as_bytes());
    k2.finalize(&mut cs_hash);
    let cs = hex::encode(cs_hash);

    let mut out = String::with_capacity(42);
    out.push_str("0x");
    for (i, c) in addr_hex.chars().enumerate() {
        let nibble = match cs.as_bytes()[i] {
            b @ b'0'..=b'9' => b - b'0',
            b @ b'a'..=b'f' => b - b'a' + 10,
            b @ b'A'..=b'F' => b - b'A' + 10,
            _ => 0,
        };
        out.push(if nibble > 7 {
            c.to_ascii_uppercase()
        } else {
            c
        });
    }
    out
}

// ── Encryption / decryption ───────────────────────────────────────────────────

/// Encrypt a UTF-8 string with AES-256-GCM (key derived via scrypt).
/// Returns a JSON-serialised [`EncryptedWallet`].
pub fn encrypt_data(data: &str, password: &str) -> Result<String> {
    use aes_gcm::{Aes256Gcm, KeyInit, aead::Aead};
    use base64::Engine;
    use rand::RngCore;
    use scrypt::scrypt;

    let mut rng = rand::thread_rng();
    let mut salt = [0u8; 16];
    rng.fill_bytes(&mut salt);
    let mut nonce_bytes = [0u8; 12];
    rng.fill_bytes(&mut nonce_bytes);

    let mut key = [0u8; 32];
    let params = scrypt::Params::new(16, 8, 1).context("Invalid scrypt params")?;
    scrypt(password.as_bytes(), &salt, &params, &mut key).context("scrypt KDF failed")?;

    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|e| anyhow::anyhow!("AES init: {e:?}"))?;
    let ciphertext = cipher
        .encrypt(&nonce_bytes.into(), data.as_bytes())
        .map_err(|e| anyhow::anyhow!("Encryption failed: {e:?}"))?;

    let b64 = base64::engine::general_purpose::STANDARD;
    serde_json::to_string_pretty(&EncryptedWallet {
        version: 1,
        salt: b64.encode(salt),
        nonce: b64.encode(nonce_bytes),
        ciphertext: b64.encode(ciphertext),
    })
    .context("Serialisation of encrypted wallet failed")
}

/// Decrypt an [`EncryptedWallet`] with AES-256-GCM and return the plaintext.
pub fn decrypt_data(enc: &EncryptedWallet, password: &str) -> Result<String> {
    use aes_gcm::{Aes256Gcm, KeyInit, aead::Aead};
    use base64::Engine;
    use scrypt::scrypt;

    if enc.version != 1 {
        anyhow::bail!("Unsupported wallet version: {}", enc.version);
    }

    let b64 = base64::engine::general_purpose::STANDARD;
    let salt = b64.decode(&enc.salt).context("Invalid salt")?;
    let nonce_vec = b64.decode(&enc.nonce).context("Invalid nonce")?;
    let ciphertext = b64.decode(&enc.ciphertext).context("Invalid ciphertext")?;
    let nonce: [u8; 12] = nonce_vec
        .try_into()
        .map_err(|_| anyhow::anyhow!("Nonce must be 12 bytes"))?;

    let mut key = [0u8; 32];
    scrypt(
        password.as_bytes(),
        &salt,
        &scrypt::Params::new(16, 8, 1)?,
        &mut key,
    )?;

    let cipher = Aes256Gcm::new_from_slice(&key)?;
    let plain = cipher
        .decrypt(&nonce.into(), ciphertext.as_ref())
        .map_err(|_| anyhow::anyhow!("Decryption failed — wrong password?"))?;

    Ok(String::from_utf8(plain)?)
}

// ── Private helpers ───────────────────────────────────────────────────────────

fn is_ed25519(chain: &str) -> bool {
    chain == "solana"
}

fn bitcoin_network(network: &str) -> Network {
    match network {
        "testnet" => Network::Testnet,
        "signet" => Network::Signet,
        "regtest" => Network::Regtest,
        _ => Network::Bitcoin,
    }
}

fn network_kind(network: Network) -> NetworkKind {
    match network {
        Network::Bitcoin => NetworkKind::Main,
        _ => NetworkKind::Test,
    }
}

/// Resolve an index specification into a concrete `Vec<u32>`.
fn resolve_indices(indexes: &Option<String>, specific: Option<u32>, num: u32) -> Result<Vec<u32>> {
    if let Some(s) = indexes {
        return parse_indexes(s);
    }
    if let Some(i) = specific {
        return Ok(vec![i]);
    }
    Ok((0..num).collect())
}

/// Assemble Solana key-info fields based on the operating mode.
fn solana_key_info(
    idx: u32,
    pubkey: &Pubkey,
    sk: [u8; 32],
    chain_code: &[u8],
    mode: &str,
    program_id: &str,
) -> Result<(String, String, Option<String>, Option<String>)> {
    match mode {
        "pda" => {
            let program = if program_id.is_empty() {
                Pubkey::from_str("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA").unwrap()
            } else {
                Pubkey::from_str(program_id).context("Invalid program ID")?
            };
            let seed_label = format!("user_deposit_{idx}");
            let seeds: &[&[u8]; 2] = &[seed_label.as_bytes(), &pubkey.to_bytes()];
            let (pda, _) = Pubkey::derive_program_address(seeds, &program)
                .context("Unable to derive Solana PDA")?;
            Ok((pda.to_string(), "PDA_RECEIVE_ONLY".to_string(), None, None))
        }
        "cold-export" => Ok((pubkey.to_string(), "HIDDEN".to_string(), None, None)),
        _ => {
            let mut xpub_bytes = chain_code.to_vec();
            xpub_bytes.extend_from_slice(&pubkey.to_bytes());
            Ok((
                pubkey.to_string(),
                hex::encode(sk),
                Some(hex::encode(sk)),
                Some(hex::encode(xpub_bytes)),
            ))
        }
    }
}

/// Parse an xpub from standard base58 BIP-32 or fallback hex `chain_code(32) || pubkey(33)`.
fn parse_xpub(s: &str) -> Result<Xpub> {
    if let Ok(x) = Xpub::from_str(s) {
        return Ok(x);
    }

    let stripped = s.strip_prefix("xpub").unwrap_or(s);
    if stripped.len() >= 64 && stripped.chars().all(|c| c.is_ascii_hexdigit()) {
        let bytes = hex::decode(stripped).context("Invalid hex xpub")?;
        if bytes.len() < 32 {
            anyhow::bail!("Hex xpub too short");
        }
        let mut chain = [0u8; 32];
        chain.copy_from_slice(&bytes[..32]);
        let cc = ChainCode::from_hex(&hex::encode(chain)).expect("32 bytes is always valid");
        let pk = bitcoin::secp256k1::PublicKey::from_slice(&bytes[32..])
            .context("Invalid public key in hex xpub")?;
        return Ok(Xpub {
            network: NetworkKind::Main,
            depth: 0,
            parent_fingerprint: Fingerprint::default(),
            child_number: ChildNumber::Normal { index: 0 },
            public_key: pk,
            chain_code: cc,
        });
    }

    anyhow::bail!("Invalid xpub: expected BIP32 base58 or hex chain_code(32)+pubkey(33)")
}
