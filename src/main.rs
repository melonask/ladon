mod config;
mod db;
mod output;
mod pool;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use ladon::{EncryptedWallet, Params, decrypt_data, derive, encrypt_data};
use output::Format;
use std::{fs, path::PathBuf};
use tracing_subscriber::{EnvFilter, fmt};

// ── CLI types ─────────────────────────────────────────────────────────────────

/// Fast multi-chain HD wallet CLI and address-pool daemon.
#[derive(Parser, Debug)]
#[command(name = "ladon", version, about, long_about = None)]
struct Cli {
    /// Path to the TOML configuration file.
    #[arg(long, short = 'C', value_name = "FILE", default_value = "Config.toml")]
    config: PathBuf,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Derive addresses and write to stdout (redirect to file as needed).
    Derive(Box<DeriveArgs>),

    /// Decrypt an encrypted wallet file.
    Decrypt(DecryptArgs),

    /// Run the address-pool daemon (requires a config file with `[database]`).
    Pool,
}

// ── derive sub-command ────────────────────────────────────────────────────────

#[derive(Parser, Debug)]
struct DeriveArgs {
    /// Target chain: `evm`, `btc`, `solana`.
    #[arg(long, short = 'c', default_value = "evm")]
    chain: String,

    /// BIP-39 mnemonic (12 or 24 words).
    #[arg(long, short = 'm')]
    mnemonic: Option<String>,

    /// BIP-39 passphrase.
    #[arg(long, default_value = "")]
    passphrase: String,

    /// Single derivation index.
    #[arg(long, short = 'i')]
    index: Option<u32>,

    /// Comma-separated indexes / ranges, e.g. `0,3,7-12`.  Overrides `--index`/`--num`.
    #[arg(long)]
    indexes: Option<String>,

    /// BIP-44 account.
    #[arg(long, default_value_t = 0)]
    account: u32,

    /// BIP-44 change.
    #[arg(long, default_value_t = 0)]
    change: u32,

    /// Number of addresses.
    #[arg(long, short = 'n', default_value_t = 1)]
    num: u32,

    /// Bitcoin network: `bitcoin`, `testnet`, `signet`, `regtest`.
    #[arg(long, default_value = "bitcoin")]
    network: String,

    /// Mnemonic word count for generation: `12` or `24`.
    #[arg(long, short = 's', default_value_t = 12)]
    strength: u32,

    /// Watch-only derivation from an xpub.
    #[arg(long)]
    xpub: Option<String>,

    /// Base path for xpub derivation.
    #[arg(long)]
    xpub_path: Option<String>,

    /// Derive from xpriv.
    #[arg(long)]
    xpriv: Option<String>,

    /// Base path for xpriv derivation.
    #[arg(long)]
    xpriv_path: Option<String>,

    /// Solana mode: `full`, `cold-export`, `hsm-sim`, `pda`.
    #[arg(long, default_value = "full")]
    solana_mode: String,

    /// Base58 program ID (PDA mode).
    #[arg(long, default_value = "")]
    program_id: String,

    /// Output format (default: json; redirect stdout for file output).
    #[arg(long, short = 'f', default_value = "json")]
    format: Format,

    /// Write output to a file instead of stdout.
    #[arg(long, short = 'o')]
    output: Option<PathBuf>,

    /// Encrypt the output with the given password.
    #[arg(long)]
    encrypt: bool,

    /// Encryption password (required when `--encrypt` is set).
    #[arg(long)]
    password: Option<String>,
}

// ── decrypt sub-command ───────────────────────────────────────────────────────

#[derive(Parser, Debug)]
struct DecryptArgs {
    /// Encrypted wallet file.
    input: PathBuf,

    /// Decryption password.
    #[arg(long)]
    password: String,
}

// ── entry point ───────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();

    match cli.cmd {
        Cmd::Derive(args) => cmd_derive(*args),
        Cmd::Decrypt(args) => cmd_decrypt(args),
        Cmd::Pool => cmd_pool(cli.config).await,
    }
}

// ── sub-command handlers ──────────────────────────────────────────────────────

fn cmd_derive(args: DeriveArgs) -> Result<()> {
    let wallet = derive(Params {
        chain: args.chain,
        mnemonic: args.mnemonic,
        passphrase: args.passphrase,
        index: args.index,
        indexes: args.indexes,
        account: args.account,
        change: args.change,
        num: args.num,
        network: args.network,
        strength: args.strength,
        xpub: args.xpub,
        xpub_path: args.xpub_path,
        xpriv: args.xpriv,
        xpriv_path: args.xpriv_path,
        solana_mode: args.solana_mode,
        program_id: args.program_id,
        ..Default::default()
    })?;

    let out = output::render(&wallet, args.format)?;

    let out = if args.encrypt {
        let password = args
            .password
            .context("--password required with --encrypt")?;
        encrypt_data(&out, &password)?
    } else {
        out
    };

    if let Some(path) = args.output {
        fs::write(&path, out).with_context(|| format!("Failed to write {}", path.display()))?;
    } else {
        print!("{out}");
    }
    Ok(())
}

fn cmd_decrypt(args: DecryptArgs) -> Result<()> {
    let raw = fs::read_to_string(&args.input)
        .with_context(|| format!("Failed to read {}", args.input.display()))?;
    let enc: EncryptedWallet = serde_json::from_str(&raw).context("Invalid encrypted wallet")?;
    let plain = decrypt_data(&enc, &args.password)?;
    print!("{plain}");
    Ok(())
}

async fn cmd_pool(config_path: PathBuf) -> Result<()> {
    let cfg = config::load(&config_path)?;
    let db = db::Db::connect(&cfg.database).await?;
    db.ensure_schema().await?;
    pool::run(&cfg, &db).await
}
