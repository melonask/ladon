use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

// ── Top-level config ──────────────────────────────────────────────────────────

/// Root configuration loaded from `Config.toml` (or the path given by `--config`).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Derivation settings (mnemonic, chains, batch size, etc.)
    pub derive: DeriveConfig,

    /// Pool-daemon settings (threshold, interval, etc.)
    #[serde(default)]
    pub pool: PoolConfig,

    /// Database backend.  Exactly one of `sqlite` or `postgres` must be set.
    pub database: DbConfig,
}

// ── Derivation ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeriveConfig {
    /// Chains to generate addresses for, e.g. `["evm", "solana"]`.
    pub chains: Vec<ChainConfig>,

    /// Where to obtain the master secret.
    pub secret: SecretSource,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChainConfig {
    /// `"evm"` | `"btc"` | `"solana"`
    pub name: String,

    /// BIP-44 account (default `0`).
    #[serde(default)]
    pub account: u32,

    /// BIP-44 change (default `0`).
    #[serde(default)]
    pub change: u32,

    /// Bitcoin network: `"bitcoin"` | `"testnet"` | `"signet"` | `"regtest"`.
    #[serde(default = "default_btc_network")]
    pub network: String,

    /// Solana derivation mode: `"full"` | `"cold-export"` | `"hsm-sim"` | `"pda"`.
    #[serde(default = "default_solana_mode")]
    pub solana_mode: String,

    /// Base58 program ID for PDA mode.
    #[serde(default)]
    pub program_id: String,
}

fn default_btc_network() -> String {
    "bitcoin".to_string()
}
fn default_solana_mode() -> String {
    "full".to_string()
}

/// How to obtain the master secret.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SecretSource {
    /// Read a BIP-39 mnemonic from an environment variable.
    Env {
        /// Environment variable name, e.g. `"LADON_MNEMONIC"`.
        var: String,
        /// Optional BIP-39 passphrase (read from this env var if set).
        #[serde(default)]
        passphrase_var: Option<String>,
    },
    /// Read a raw hex xpriv from an environment variable.
    XprivEnv { var: String },
    /// Read a BIP-39 mnemonic from a file (newline-terminated).
    File {
        path: String,
        #[serde(default)]
        passphrase_var: Option<String>,
    },
}

// ── Pool daemon ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PoolConfig {
    /// Target number of pre-generated addresses to keep in the pool.
    #[serde(default = "default_target")]
    pub target: u32,

    /// Refill when the pool drops below this count.
    #[serde(default = "default_threshold")]
    pub threshold: u32,

    /// How many addresses to generate in one batch.
    #[serde(default = "default_batch")]
    pub batch: u32,

    /// Poll interval in seconds.
    #[serde(default = "default_interval")]
    pub interval_secs: u64,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            target: default_target(),
            threshold: default_threshold(),
            batch: default_batch(),
            interval_secs: default_interval(),
        }
    }
}

fn default_target() -> u32 {
    1000
}
fn default_threshold() -> u32 {
    200
}
fn default_batch() -> u32 {
    100
}
fn default_interval() -> u64 {
    10
}

// ── Database ──────────────────────────────────────────────────────────────────

/// Exactly one backend must be present.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DbConfig {
    pub sqlite: Option<SqliteConfig>,
    pub postgres: Option<PostgresConfig>,
}

impl DbConfig {
    /// Validate that exactly one backend is configured.
    pub fn validate(&self) -> Result<()> {
        match (&self.sqlite, &self.postgres) {
            (Some(_), None) | (None, Some(_)) => Ok(()),
            (None, None) => {
                anyhow::bail!("database: at least one of `sqlite` or `postgres` must be set")
            }
            (Some(_), Some(_)) => {
                anyhow::bail!("database: only one of `sqlite` or `postgres` may be set")
            }
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SqliteConfig {
    /// Path to the SQLite file, e.g. `"data/addresses.db"`.
    pub path: String,
    pub table: TableConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PostgresConfig {
    /// `postgres://user:password@host:5432/dbname`
    /// May also be set via the `DATABASE_URL` environment variable; this field
    /// takes precedence when both are present.
    pub url: Option<String>,

    /// Env var that holds the connection URL (alternative to inline `url`).
    #[serde(default)]
    pub url_env: Option<String>,

    /// Connection-pool size (default: 5).
    #[serde(default = "default_pool_size")]
    pub pool_size: u32,

    pub table: TableConfig,
}

fn default_pool_size() -> u32 {
    5
}

impl PostgresConfig {
    /// Resolve the connection URL from inline value or env var.
    pub fn url(&self) -> Result<String> {
        if let Some(u) = &self.url {
            return Ok(u.clone());
        }
        if let Some(var) = &self.url_env {
            return std::env::var(var).with_context(|| format!("env var `{var}` not set"));
        }
        std::env::var("DATABASE_URL").context("No postgres URL: set `database.postgres.url`, `database.postgres.url_env`, or `DATABASE_URL`")
    }
}

/// Column / table names written to the database.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TableConfig {
    pub name: String,
    pub columns: ColumnConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ColumnConfig {
    /// Primary key column (ULID text).
    #[serde(default = "col_id")]
    pub id: String,

    /// Chain identifier column, e.g. `"evm"`.
    #[serde(default = "col_chain")]
    pub chain: String,

    /// Address column.
    #[serde(default = "col_address")]
    pub address: String,

    /// Derivation-path column.
    #[serde(default = "col_path")]
    pub path: String,

    /// HD index column.
    #[serde(default = "col_index")]
    pub index: String,

    /// ISO-8601 creation timestamp column.
    #[serde(default = "col_created_at")]
    pub created_at: String,
}

fn col_id() -> String {
    "id".to_string()
}
fn col_chain() -> String {
    "chain".to_string()
}
fn col_address() -> String {
    "address".to_string()
}
fn col_path() -> String {
    "path".to_string()
}
fn col_index() -> String {
    "index".to_string()
}
fn col_created_at() -> String {
    "created_at".to_string()
}

// ── Loader ────────────────────────────────────────────────────────────────────

/// Load and validate a [`Config`] from a TOML file.
pub fn load(path: &Path) -> Result<Config> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read config: {}", path.display()))?;
    let cfg: Config = toml::from_str(&raw)
        .with_context(|| format!("Failed to parse config: {}", path.display()))?;
    cfg.database.validate()?;
    Ok(cfg)
}
