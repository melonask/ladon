mod config;
mod db;
mod pool;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing_subscriber::{EnvFilter, fmt};

// ── CLI types ─────────────────────────────────────────────────────────────────

/// Fast multi-chain HD wallet CLI and address-pool daemon.
#[derive(Parser, Debug)]
#[command(name = "ladon", version, about, long_about = None)]
struct Cli {
    /// Path to the TOML configuration file.
    #[arg(long, value_name = "FILE", env = "LADON_CONFIG")]
    config: Option<PathBuf>,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Validate the universal configuration without printing secret values.
    Check,

    /// Run the address-pool daemon (requires a config file with `[ladon]`).
    Pool,
}

// ── entry point ───────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();

    let config_path = cli
        .config
        .context("--config FILE or LADON_CONFIG is required")?;
    match cli.cmd {
        Cmd::Check => cmd_check(config_path),
        Cmd::Pool => cmd_pool(config_path).await,
    }
}

fn cmd_check(config_path: PathBuf) -> Result<()> {
    let report = config::check(&config_path)?;
    println!(
        "configuration valid: store_driver={}, table={}, chains={}, pool_target={}",
        report.store_driver, report.table, report.chain_count, report.pool_target
    );
    Ok(())
}

// ── sub-command handlers ──────────────────────────────────────────────────────

async fn cmd_pool(config_path: PathBuf) -> Result<()> {
    let cfg = config::load(&config_path)?;
    let db = db::Db::connect(&cfg.database).await?;
    db.ensure_schema().await?;
    pool::run(&cfg, &db).await
}
