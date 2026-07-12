use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

// ── Environment variable expansion ───────────────────────────────────────────

/// Expand `${VAR}` and `${VAR:-default}` placeholders in the raw TOML string.
///
/// * `${VAR}` — replaced with the value of `VAR`. Fails if `VAR` is not set.
/// * `${VAR:-default}` — replaced with the value of `VAR` if set, otherwise `default`.
///
/// The error message names the variable and, when available, the closest
/// enclosing config path for easier diagnosis.
fn expand_env(raw: &str) -> Result<String> {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    let mut pos: usize = 0;

    while let Some(dollar) = rest.find('$') {
        // Copy everything before the `$`.
        out.push_str(&rest[..dollar]);
        let after_dollar = &rest[dollar + 1..];
        pos += dollar;

        if after_dollar.starts_with('{') {
            // Find the closing `}`.
            let close = after_dollar.find('}').ok_or_else(|| {
                anyhow::anyhow!("Unclosed ${{ at byte {pos}: environment expansion missing '}}'")
            })?;
            let inner = &after_dollar[1..close]; // content between { }
            let remaining = &after_dollar[close + 1..];
            pos += close + 1;

            if let Some((var_name, default)) = inner.split_once(":-") {
                let var_name = var_name.trim();
                let default = default.trim();
                if var_name.is_empty() {
                    anyhow::bail!("Empty variable name in environment expansion at byte {pos}");
                }
                match std::env::var(var_name) {
                    Ok(val) => out.push_str(&val),
                    Err(_) => out.push_str(default),
                }
            } else {
                let var_name = inner.trim();
                if var_name.is_empty() {
                    anyhow::bail!("Empty variable name in environment expansion at byte {pos}");
                }
                let val = std::env::var(var_name).with_context(|| {
                    format!("Environment variable `{var_name}` is not set (referenced at byte ~{pos} in config)")
                })?;
                out.push_str(&val);
            }
            rest = remaining;
        } else {
            // Escaped or literal `$` — keep it.
            out.push('$');
            rest = after_dollar;
            pos += 1;
        }
    }
    out.push_str(rest);
    Ok(out)
}

// ── Top-level config (the public-facing type used by existing code) ──────────

/// Root configuration loaded from a universal TOML file.
///
/// Built by [`load`] from a configuration with `[ladon]` and `[stores]` sections.
#[derive(Debug, Clone)]
pub struct Config {
    /// Derivation settings (mnemonic, chains, batch size, etc.)
    pub derive: DeriveConfig,

    /// Pool-daemon settings (threshold, interval, etc.)
    pub pool: PoolConfig,

    /// Database backend. Exactly one of `sqlite` or `postgres` will be set.
    pub database: DbConfig,
}

/// Non-sensitive summary returned by [`check`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckReport {
    pub store_driver: &'static str,
    pub table: String,
    pub chain_count: usize,
    pub pool_target: u32,
}

// ── Derivation ───────────────────────────────────────────────────────────────

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

    /// Shared chain id from `[chains.<id>]`.
    pub chain: String,

    /// BIP-44 account (default `0`).
    #[serde(default)]
    pub account: u32,

    /// BIP-44 change (default `0`).
    #[serde(default)]
    pub change: u32,

    /// First address index to generate when no rows exist for this chain (default `0`).
    #[serde(default)]
    pub start_index: u32,

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

// ── Pool daemon ──────────────────────────────────────────────────────────────

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

// ── Database ─────────────────────────────────────────────────────────────────

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
    /// Resolved URL from the required `[stores.<id>].url` environment reference.
    pub url: String,

    /// Connection-pool size (default: 5).
    #[serde(default = "default_pool_size")]
    pub pool_size: u32,

    pub table: TableConfig,
}

fn default_pool_size() -> u32 {
    5
}

/// Column and table identifiers written to the database.
///
/// # Safety
/// SQL parameters cannot bind identifiers. These values cross Ladon's only
/// dynamic-SQL trust boundary and are accepted only after ASCII identifier
/// validation in [`validate_table_config`].
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

    /// Nullable boolean usage marker. NULL means available; TRUE means used.
    #[serde(default = "col_is_used")]
    pub is_used: String,

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
fn col_is_used() -> String {
    "is_used".to_string()
}
fn col_created_at() -> String {
    "created_at".to_string()
}

// ── Internal deserialisation helpers for the loader ──────────────────────────

/// The `[ladon]` namespace as it appears in a universal TOML.
///
/// Parsed with `deny_unknown_fields` so any stray key is rejected early.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct LadonNamespace {
    /// Store id from `[stores]` used by pool mode.
    store: String,

    #[serde(default)]
    derive: Option<DeriveConfig>,

    #[serde(default)]
    pool: Option<PoolConfig>,

    #[serde(default)]
    table: Option<TableConfig>,
}

/// A single `[stores.<id>]` entry.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoreEntry {
    driver: String,
    url: String,
    #[serde(default = "default_pool_size")]
    max_connections: u32,
}

// ── Loader ───────────────────────────────────────────────────────────────────

/// Load and validate a [`Config`] from a TOML file.
///
/// Supports two formats:
///
/// **Universal merged config** — the recommended format for multi-package
/// deployments.  The file contains shared root sections (`[stores]`, `[paths]`,
/// …) plus a `[ladon]` namespace:
///
/// ```toml
/// [stores.ladon]
/// driver = "sqlite"
/// url = "sqlite://data/ladon/addresses.db"
///
/// [ladon]
/// store = "ladon"
///
/// [ladon.derive.secret]
/// kind = "env"
/// var = "LADON_MNEMONIC"
///
/// [[ladon.derive.chains]]
/// name = "evm"
/// ```
///
pub fn load(path: &Path) -> Result<Config> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read config: {}", path.display()))?;

    // 1. Expand environment variables.
    let expanded = expand_env(&raw)
        .with_context(|| format!("Environment expansion failed in {}", path.display()))?;

    // Parse the root loosely so independently owned namespaces remain forward-compatible.
    let root: toml::Value = toml::from_str(&expanded)
        .map_err(|_| anyhow::anyhow!("Failed to parse config: {}", path.display()))?;

    // 3. Extract the `[ladon]` namespace and parse it strictly.
    let ladon_val = root
        .get("ladon")
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("Missing required [ladon] section in {}", path.display()))?;

    // Re-serialise the ladon sub-table so we can parse it with serde +
    // deny_unknown_fields.  This catches stray keys inside [ladon].
    let ladon_toml =
        toml::to_string_pretty(&ladon_val).context("Failed to re-serialise [ladon] section")?;

    let ladon: LadonNamespace = toml::from_str(&ladon_toml).map_err(|_| {
        anyhow::anyhow!(
            "Invalid [ladon] section in {} — unknown or malformed fields",
            path.display()
        )
    })?;

    // 4. Resolve the store.
    let store_id = &ladon.store;
    let stores_table = root
        .get("stores")
        .and_then(toml::Value::as_table)
        .ok_or_else(|| anyhow::anyhow!("Missing required [stores] section"))?;
    let store_value = stores_table.get(store_id).ok_or_else(|| {
        anyhow::anyhow!("[ladon].store `{store_id}` does not reference an entry in [stores]")
    })?;
    let store: StoreEntry = toml::to_string_pretty(store_value)
        .ok()
        .and_then(|value| toml::from_str(&value).ok())
        .ok_or_else(|| anyhow::anyhow!("Invalid [stores.{store_id}] section"))?;
    validate_store_reference(&raw, store_id, &store)?;

    // 5. Build the internal DbConfig.
    let table = ladon.table.clone().unwrap_or(TableConfig {
        name: "derived_addresses".to_string(),
        columns: ColumnConfig {
            id: col_id(),
            chain: col_chain(),
            address: col_address(),
            path: col_path(),
            index: col_index(),
            is_used: col_is_used(),
            created_at: col_created_at(),
        },
    });

    let database = build_db_config(store, store_id, &table)?;

    // 6. Build the internal DeriveConfig and PoolConfig.
    let derive = ladon
        .derive
        .ok_or_else(|| anyhow::anyhow!("Missing required [ladon.derive] section"))?;

    // 7. Validate required universal chain references.
    let shared_chain_ids = root
        .get("chains")
        .and_then(|v| v.as_table())
        .map(|tbl| {
            tbl.keys()
                .cloned()
                .collect::<std::collections::HashSet<_>>()
        })
        .unwrap_or_default();

    for chain_cfg in &derive.chains {
        if !shared_chain_ids.contains(&chain_cfg.chain) {
            anyhow::bail!(
                "chain reference `{}` in [[ladon.derive.chains]] for local chain `{}` not found in [chains.{}]",
                chain_cfg.chain,
                chain_cfg.name,
                chain_cfg.chain
            );
        }
    }

    let pool = ladon
        .pool
        .ok_or_else(|| anyhow::anyhow!("Missing required [ladon.pool] section"))?;

    let cfg = Config {
        derive,
        pool,
        database,
    };
    cfg.database.validate()?;
    validate_operation_config(&cfg)?;
    Ok(cfg)
}

/// Validate a universal configuration and verify that declared secret references
/// are available. The returned report intentionally contains no secret material.
pub fn check(path: &Path) -> Result<CheckReport> {
    let cfg = load(path)?;
    validate_secret_reference(&cfg.derive.secret)?;
    let (store_driver, table) = match &cfg.database {
        DbConfig {
            sqlite: Some(sqlite),
            ..
        } => ("sqlite", sqlite.table.name.clone()),
        DbConfig {
            postgres: Some(postgres),
            ..
        } => ("postgres", postgres.table.name.clone()),
        _ => anyhow::bail!("database backend validation failed"),
    };
    Ok(CheckReport {
        store_driver,
        table,
        chain_count: cfg.derive.chains.len(),
        pool_target: cfg.pool.target,
    })
}

fn validate_store_reference(raw: &str, store_id: &str, store: &StoreEntry) -> Result<()> {
    if store.driver != "postgres" {
        return Ok(());
    }
    let raw_root: toml::Value = toml::from_str(raw).map_err(|_| {
        anyhow::anyhow!("Failed to parse configuration while validating store references")
    })?;
    let url = raw_root
        .get("stores")
        .and_then(toml::Value::as_table)
        .and_then(|stores| stores.get(store_id))
        .and_then(toml::Value::as_table)
        .and_then(|store| store.get("url"))
        .and_then(toml::Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("[stores.{store_id}].url must be a string"))?;
    let Some(variable) = url
        .strip_prefix("${")
        .and_then(|value| value.strip_suffix('}'))
    else {
        anyhow::bail!(
            "[stores.{store_id}].url must be an environment reference such as `${{DATABASE_URL}}`; inline PostgreSQL credentials are not allowed"
        );
    };
    if variable.is_empty() || variable.contains(":-") || !is_env_name(variable) {
        anyhow::bail!(
            "[stores.{store_id}].url must reference one environment variable without a default"
        );
    }
    Ok(())
}

fn validate_operation_config(cfg: &Config) -> Result<()> {
    if cfg.derive.chains.is_empty() {
        anyhow::bail!("[ladon.derive.chains] must contain at least one chain")
    }
    if cfg.pool.batch == 0
        || cfg.pool.target == 0
        || cfg.pool.threshold > cfg.pool.target
        || cfg.pool.interval_secs == 0
    {
        anyhow::bail!(
            "[ladon.pool] requires target > 0, batch > 0, interval_secs > 0, and threshold <= target"
        )
    }
    let mut names = std::collections::HashSet::new();
    for chain in &cfg.derive.chains {
        let canonical = ladon::canonical_chain(&chain.name)?;
        if !names.insert(canonical) {
            anyhow::bail!(
                "[ladon.derive.chains] contains duplicate chain `{}`",
                chain.name
            )
        }
    }
    Ok(())
}

fn validate_secret_reference(source: &SecretSource) -> Result<()> {
    match source {
        SecretSource::Env {
            var,
            passphrase_var,
        } => {
            std::env::var_os(var)
                .ok_or_else(|| anyhow::anyhow!("Secret environment variable `{var}` is not set"))?;
            if let Some(passphrase_var) = passphrase_var {
                std::env::var_os(passphrase_var).ok_or_else(|| {
                    anyhow::anyhow!("Passphrase environment variable `{passphrase_var}` is not set")
                })?;
            }
        }
        SecretSource::XprivEnv { var } => {
            std::env::var_os(var)
                .ok_or_else(|| anyhow::anyhow!("Secret environment variable `{var}` is not set"))?;
        }
        SecretSource::File {
            path,
            passphrase_var,
        } => {
            std::fs::File::open(path)
                .with_context(|| format!("Cannot open secret file `{path}`"))?;
            if let Some(passphrase_var) = passphrase_var {
                std::env::var_os(passphrase_var).ok_or_else(|| {
                    anyhow::anyhow!("Passphrase environment variable `{passphrase_var}` is not set")
                })?;
            }
        }
    }
    Ok(())
}

fn is_env_name(value: &str) -> bool {
    value.bytes().enumerate().all(|(index, byte)| {
        byte == b'_' || byte.is_ascii_uppercase() || (index > 0 && byte.is_ascii_digit())
    })
}

/// Build a [`DbConfig`] from a resolved [`StoreEntry`] and a [`TableConfig`].
fn build_db_config(store: StoreEntry, store_id: &str, table: &TableConfig) -> Result<DbConfig> {
    validate_table_config(table)?;

    match store {
        s if s.driver == "sqlite" => {
            // Extract the file path from the SQLite URL.  sqlx expects
            // `sqlite://path` where path may be absolute or relative.
            let path = s
                .url
                .strip_prefix("sqlite://")
                .unwrap_or(&s.url)
                .to_string();
            Ok(DbConfig {
                sqlite: Some(SqliteConfig {
                    path,
                    table: table.clone(),
                }),
                postgres: None,
            })
        }
        s if s.driver == "postgres" => {
            // Feature-gate check: postgres requires the `pg` or `postgres` feature.
            if !(cfg!(feature = "postgres") || cfg!(feature = "pg")) {
                anyhow::bail!(
                    "Store `{store_id}` requires driver `postgres`, \
                     but the `pg`/`postgres` feature is not enabled. \
                     Recompile with `--features pg` or `--features postgres`."
                );
            }
            Ok(DbConfig {
                sqlite: None,
                postgres: Some(PostgresConfig {
                    url: s.url,
                    pool_size: s.max_connections,
                    table: table.clone(),
                }),
            })
        }
        s => {
            anyhow::bail!(
                "Store `{store_id}` has unsupported driver `{}`. Use `sqlite` or `postgres`.",
                s.driver
            )
        }
    }
}

fn validate_table_config(table: &TableConfig) -> Result<()> {
    validate_sql_identifier("ladon.table.name", &table.name)?;
    validate_sql_identifier("ladon.table.columns.id", &table.columns.id)?;
    validate_sql_identifier("ladon.table.columns.chain", &table.columns.chain)?;
    validate_sql_identifier("ladon.table.columns.address", &table.columns.address)?;
    validate_sql_identifier("ladon.table.columns.path", &table.columns.path)?;
    validate_sql_identifier("ladon.table.columns.index", &table.columns.index)?;
    validate_sql_identifier("ladon.table.columns.is_used", &table.columns.is_used)?;
    validate_sql_identifier("ladon.table.columns.created_at", &table.columns.created_at)?;
    Ok(())
}

/// Validate an identifier before it is interpolated into Ladon's SQL templates.
///
/// This is the sole identifier trust boundary: database values are always bound
/// separately, while schema identifiers must be ASCII letters, digits, or
/// underscores and begin with a letter or underscore.
fn validate_sql_identifier(path: &str, value: &str) -> Result<()> {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        anyhow::bail!("{path} must be a non-empty SQL identifier");
    };
    if !(first == '_' || first.is_ascii_alphabetic()) {
        anyhow::bail!(
            "{path} has invalid SQL identifier `{value}`; identifiers must start with a letter or underscore"
        );
    }
    if !chars.all(|c| c == '_' || c.is_ascii_alphanumeric()) {
        anyhow::bail!(
            "{path} has invalid SQL identifier `{value}`; use only ASCII letters, digits, and underscores"
        );
    }
    Ok(())
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── env expansion ────────────────────────────────────────────────────

    #[test]
    fn expand_env_no_placeholders_is_identity() {
        let input = "hello world";
        assert_eq!(expand_env(input).unwrap(), input);
    }

    #[test]
    fn expand_env_simple_var() {
        unsafe { std::env::set_var("LADON_TEST_VAR_A", "my_value") };
        let result = expand_env("url = \"${LADON_TEST_VAR_A}\"").unwrap();
        assert!(result.contains("my_value"));
        unsafe { std::env::remove_var("LADON_TEST_VAR_A") };
    }

    #[test]
    fn expand_env_var_with_default_uses_var_when_set() {
        unsafe { std::env::set_var("LADON_TEST_VAR_B", "set_value") };
        let result = expand_env("x = \"${LADON_TEST_VAR_B:-fallback}\"").unwrap();
        assert!(result.contains("set_value"));
        assert!(!result.contains("fallback"));
        unsafe { std::env::remove_var("LADON_TEST_VAR_B") };
    }

    #[test]
    fn expand_env_var_with_default_falls_back() {
        unsafe { std::env::remove_var("LADON_TEST_VAR_MISSING") };
        let result = expand_env("x = \"${LADON_TEST_VAR_MISSING:-hello}\"").unwrap();
        assert!(result.contains("hello"));
    }

    #[test]
    fn expand_env_missing_var_without_default_fails() {
        unsafe { std::env::remove_var("LADON_TEST_VAR_SHOULD_NOT_EXIST") };
        let err = expand_env("x = \"${LADON_TEST_VAR_SHOULD_NOT_EXIST}\"").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("LADON_TEST_VAR_SHOULD_NOT_EXIST"), "{}", msg);
        assert!(msg.contains("not set"), "{}", msg);
    }

    #[test]
    fn expand_env_multiple_placeholders() {
        unsafe {
            std::env::set_var("LADON_V1", "alpha");
            std::env::set_var("LADON_V2", "beta");
        }
        let result = expand_env("a = \"${LADON_V1}_${LADON_V2}_${LADON_V3:-gamma}\"").unwrap();
        assert_eq!(result, "a = \"alpha_beta_gamma\"");
        unsafe {
            std::env::remove_var("LADON_V1");
            std::env::remove_var("LADON_V2");
        }
    }

    #[test]
    fn expand_env_empty_var_name_fails() {
        let err = expand_env("x = \"${}\"").unwrap_err();
        assert!(err.to_string().contains("Empty variable name"));
    }

    #[test]
    fn expand_env_empty_var_name_with_default_fails() {
        let err = expand_env("x = \"${:-default}\"").unwrap_err();
        assert!(err.to_string().contains("Empty variable name"));
    }

    // ── store resolution ─────────────────────────────────────────────────

    #[test]
    fn build_db_config_sqlite_store() {
        let store = StoreEntry {
            driver: "sqlite".to_string(),
            url: "sqlite://data/ladon/addresses.db".to_string(),
            max_connections: 1,
        };
        let table = default_table();
        let db = build_db_config(store, "ladon", &table).unwrap();
        db.validate().unwrap();
        assert!(db.sqlite.is_some());
        assert!(db.postgres.is_none());
        assert_eq!(db.sqlite.as_ref().unwrap().path, "data/ladon/addresses.db");
        assert_eq!(db.sqlite.as_ref().unwrap().table.name, "derived_addresses");
    }

    #[test]
    fn build_db_config_postgres_store_without_feature_fails() {
        // This test must only run when the pg/postgres feature is *not* enabled.
        if cfg!(feature = "postgres") || cfg!(feature = "pg") {
            return; // skip — feature is on, so we expect success, not failure
        }
        let store = StoreEntry {
            driver: "postgres".to_string(),
            url: "postgres://user:pass@localhost/ladon".to_string(),
            max_connections: 5,
        };
        let table = default_table();
        let err = build_db_config(store, "ladon", &table).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("pg") || msg.contains("postgres"),
            "Error should mention pg/postgres feature: {msg}"
        );
        assert!(msg.contains("not enabled"), "{msg}");
    }

    #[test]
    fn build_db_config_postgres_store_with_feature_succeeds() {
        if !(cfg!(feature = "postgres") || cfg!(feature = "pg")) {
            return; // skip — feature is off
        }
        let store = StoreEntry {
            driver: "postgres".to_string(),
            url: "postgres://user:pass@localhost/ladon".to_string(),
            max_connections: 5,
        };
        let table = default_table();
        let db = build_db_config(store, "ladon", &table).unwrap();
        db.validate().unwrap();
        assert!(db.postgres.is_some());
        assert!(db.sqlite.is_none());
    }

    #[test]
    fn build_db_config_unknown_driver_fails() {
        let store = StoreEntry {
            driver: "mysql".to_string(),
            url: "mysql://localhost/db".to_string(),
            max_connections: 1,
        };
        let table = default_table();
        let err = build_db_config(store, "my_store", &table).unwrap_err();
        assert!(err.to_string().contains("unsupported driver"));
        assert!(err.to_string().contains("mysql"));
    }

    #[test]
    fn build_db_config_rejects_unsafe_sql_identifier() {
        let mut table = default_table();
        table.name = "addresses; DROP TABLE addresses".to_string();

        let err = build_db_config(
            StoreEntry {
                driver: "sqlite".to_string(),
                url: "sqlite://data/ladon/addresses.db".to_string(),
                max_connections: 1,
            },
            "ladon",
            &table,
        )
        .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("ladon.table.name"), "{msg}");
        assert!(msg.contains("SQL identifier"), "{msg}");
    }

    // ── ladon namespace parsing ──────────────────────────────────────────

    #[test]
    fn ladon_namespace_denies_unknown_fields() {
        let toml_str = r#"
            enabled = true
            store = "ladon"
            unknown_key = "should fail"
        "#;
        let err: Result<LadonNamespace, _> = toml::from_str(toml_str);
        assert!(err.is_err());
        let msg = err.unwrap_err().to_string();
        assert!(
            msg.contains("unknown_key") || msg.contains("unknown field"),
            "{msg}"
        );
    }

    #[test]
    fn ladon_namespace_allows_known_fields() {
        let toml_str = r#"
            store = "mystore"

            [derive]
            [derive.secret]
            kind = "env"
            var = "MY_MNEMONIC"

            [[derive.chains]]
            name = "evm"
            chain = "ethereum-mainnet"

            [pool]
            target = 500
            threshold = 100
            batch = 50
            interval_secs = 30

            [table]
            name = "addrs"
            [table.columns]
            id = "pk"
            chain = "ch"
            address = "addr"
            path = "p"
            index = "idx"
            is_used = "used"
            created_at = "ts"
        "#;
        let ladon: LadonNamespace = toml::from_str(toml_str).unwrap();
        assert_eq!(ladon.store, "mystore");
        assert!(ladon.derive.is_some());
        assert!(ladon.pool.is_some());
        assert!(ladon.table.is_some());
    }

    // ── top-level load with merged config ────────────────────────────────

    #[test]
    fn load_merged_config_with_ladon_and_stores() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("ladon-test-merged-{}.toml", std::process::id()));

        let toml = r#"
            version = 1

            [meta]
            name = "test-stack"

            [stores.ladon]
            driver = "sqlite"
            url = "sqlite://data/ladon/test.db"

            [ladon]
            store = "ladon"

            [ladon.derive.secret]
            kind = "env"
            var = "LADON_MNEMONIC"

            [[ladon.derive.chains]]
            name = "evm"
            chain = "ethereum-mainnet"

            [chains.ethereum-mainnet]
            caip2 = "eip155:1"

            [ladon.pool]
            target = 100
            threshold = 50

            [ladon.table]
            name = "test_addrs"

            [ladon.table.columns]
            id = "id"
            chain = "chain"
            address = "address"
            path = "path"
            index = "index"
            is_used = "is_used"
            created_at = "created_at"

            # Unrelated package namespace — should be ignored.
            [pano]
            enabled = true

            [bria]
            enabled = true
        "#;
        std::fs::write(&path, toml).unwrap();
        let cfg = load(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        assert_eq!(cfg.derive.chains.len(), 1);
        assert_eq!(cfg.derive.chains[0].name, "evm");
        assert_eq!(cfg.pool.target, 100);
        assert!(cfg.database.sqlite.is_some());
        assert_eq!(
            cfg.database.sqlite.as_ref().unwrap().table.name,
            "test_addrs"
        );
    }

    #[test]
    fn load_ignores_unrelated_namespaces() {
        // pano and bria namespaces inside the root should not cause errors.
        let dir = std::env::temp_dir();
        let path = dir.join(format!("ladon-test-ignore-{}.toml", std::process::id()));

        let toml = r#"
            [stores.ladon]
            driver = "sqlite"
            url = "sqlite://data/test.db"

            [ladon]
            store = "ladon"

            [ladon.derive.secret]
            kind = "env"
            var = "LADON_MNEMONIC"

            [[ladon.derive.chains]]
            name = "evm"
            chain = "ethereum-mainnet"

            [ladon.pool]
            target = 10
            threshold = 5
            batch = 5
            interval_secs = 1

            [chains.ethereum-mainnet]
            caip2 = "eip155:1"

            [pano.server]
            enabled = true

            [bria.global]
            worker_threads = 4

            [oracles.safety]
            enabled = true
        "#;
        std::fs::write(&path, toml).unwrap();
        let result = load(&path);
        let _ = std::fs::remove_file(&path);
        assert!(
            result.is_ok(),
            "Should ignore unrelated namespaces: {:?}",
            result.err()
        );
    }

    #[test]
    fn load_rejects_unknown_field_in_ladon_namespace() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("ladon-test-unknown-{}.toml", std::process::id()));

        let toml = r#"
            [ladon]
            store = "ladon"
            sausage = "bad"

            [ladon.derive.secret]
            kind = "env"
            var = "LADON_MNEMONIC"
        "#;
        std::fs::write(&path, toml).unwrap();
        let result = load(&path);
        let _ = std::fs::remove_file(&path);
        assert!(result.is_err(), "Should reject unknown field in [ladon]");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("sausage") || msg.contains("unknown"),
            "Error should mention the unknown field: {msg}"
        );
    }

    #[test]
    fn load_rejects_unknown_field_in_ladon_derive() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!(
            "ladon-test-unknown-derive-{}.toml",
            std::process::id()
        ));

        let toml = r#"
            [ladon.derive]
            format = "json"
            strength = 12
            bad_field = "nope"
            chains = []
            [ladon.derive.secret]
            kind = "env"
            var = "LADON_MNEMONIC"
        "#;
        std::fs::write(&path, toml).unwrap();
        let result = load(&path);
        let _ = std::fs::remove_file(&path);
        assert!(
            result.is_err(),
            "Should reject unknown field in [ladon.derive]"
        );
    }

    #[test]
    fn load_env_expansion_in_config() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("ladon-test-env-expand-{}.toml", std::process::id()));

        unsafe { std::env::set_var("LADON_TEST_MNEMONIC_VAR", "MY_MNEMONIC_VAR_NAME") };
        let toml = r#"
            [stores.ladon]
            driver = "sqlite"
            url = "sqlite://data/test.db"

            [ladon]
            store = "ladon"

            [ladon.derive.secret]
            kind = "env"
            var = "${LADON_TEST_MNEMONIC_VAR}"

            [[ladon.derive.chains]]
            name = "evm"
            chain = "ethereum-mainnet"

            [ladon.pool]
            target = 10
            threshold = 5
            batch = 5
            interval_secs = 1

            [chains.ethereum-mainnet]
            caip2 = "eip155:1"
        "#;
        std::fs::write(&path, toml).unwrap();
        let cfg = load(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        unsafe { std::env::remove_var("LADON_TEST_MNEMONIC_VAR") };

        match &cfg.derive.secret {
            SecretSource::Env { var, .. } => {
                assert_eq!(var, "MY_MNEMONIC_VAR_NAME");
            }
            _ => panic!("Expected Env secret source"),
        }
    }

    #[test]
    fn load_env_expansion_with_default_in_config() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!(
            "ladon-test-env-default-{}.toml",
            std::process::id()
        ));

        unsafe { std::env::remove_var("LADON_NONEXISTENT_FOR_TEST") };
        let toml = r#"
            [stores.ladon]
            driver = "sqlite"
            url = "sqlite://data/test.db"

            [ladon]
            store = "ladon"

            [ladon.derive.secret]
            kind = "env"
            var = "${LADON_NONEXISTENT_FOR_TEST:-DEFAULT_MNEMONIC_VAR}"

            [[ladon.derive.chains]]
            name = "evm"
            chain = "ethereum-mainnet"

            [ladon.pool]
            target = 10
            threshold = 5
            batch = 5
            interval_secs = 1

            [chains.ethereum-mainnet]
            caip2 = "eip155:1"
        "#;
        std::fs::write(&path, toml).unwrap();
        let cfg = load(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        match &cfg.derive.secret {
            SecretSource::Env { var, .. } => {
                assert_eq!(var, "DEFAULT_MNEMONIC_VAR");
            }
            _ => panic!("Expected Env secret source"),
        }
    }

    #[test]
    fn load_rejects_missing_shared_store() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("ladon-test-no-store-{}.toml", std::process::id()));

        let toml = r#"
            [ladon]
            store = "missing"

            [ladon.derive.secret]
            kind = "env"
            var = "LADON_MNEMONIC"

            [[ladon.derive.chains]]
            name = "evm"
            chain = "ethereum-mainnet"

            [ladon.pool]
            target = 10
            threshold = 5
            batch = 5
            interval_secs = 1

            [ladon.table]
            name = "my_table"

            [ladon.table.columns]
            id = "id"
            chain = "chain"
            address = "address"
            path = "path"
            index = "index"
            is_used = "is_used"
            created_at = "created_at"
        "#;
        std::fs::write(&path, toml).unwrap();
        let result = load(&path);
        let _ = std::fs::remove_file(&path);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("[stores]"));
    }

    #[test]
    fn check_reports_only_non_secret_configuration() {
        let path =
            std::env::temp_dir().join(format!("ladon-test-check-{}.toml", std::process::id()));
        unsafe { std::env::set_var("LADON_TEST_CHECK_SECRET", "must-not-appear") };
        std::fs::write(
            &path,
            r#"
            [stores.ladon]
            driver = "sqlite"
            url = "sqlite://data/test.db"

            [ladon]
            store = "ladon"

            [ladon.derive.secret]
            kind = "env"
            var = "LADON_TEST_CHECK_SECRET"

            [[ladon.derive.chains]]
            name = "evm"
            chain = "ethereum-mainnet"

            [ladon.pool]
            target = 10
            threshold = 5
            batch = 5
            interval_secs = 1

            [chains.ethereum-mainnet]
            caip2 = "eip155:1"
            "#,
        )
        .unwrap();

        let report = check(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        unsafe { std::env::remove_var("LADON_TEST_CHECK_SECRET") };

        assert_eq!(report.store_driver, "sqlite");
        assert!(!format!("{report:?}").contains("must-not-appear"));
    }

    #[test]
    fn load_rejects_inline_postgres_credentials_before_connection() {
        let path =
            std::env::temp_dir().join(format!("ladon-test-pg-secret-{}.toml", std::process::id()));
        std::fs::write(
            &path,
            r#"
            [stores.ladon]
            driver = "postgres"
            url = "postgres://user:password@host/ladon"

            [ladon]
            store = "ladon"

            [ladon.derive.secret]
            kind = "env"
            var = "LADON_MNEMONIC"

            [[ladon.derive.chains]]
            name = "evm"
            chain = "ethereum-mainnet"

            [ladon.pool]
            target = 10
            threshold = 5
            batch = 5
            interval_secs = 1

            [chains.ethereum-mainnet]
            caip2 = "eip155:1"
            "#,
        )
        .unwrap();
        let error = load(&path).unwrap_err();
        let _ = std::fs::remove_file(&path);

        assert!(error.to_string().contains("environment reference"));
        assert!(!error.to_string().contains("password"));
    }

    #[test]
    fn check_accepts_complete_universal_config_without_secret_output() {
        let path =
            std::env::temp_dir().join(format!("ladon-universal-{}.toml", std::process::id()));
        unsafe { std::env::set_var("LADON_TEST_UNIVERSAL_SECRET", "not-in-report") };
        std::fs::write(
            &path,
            r#"
[stores.shared]
driver = "sqlite"
url = "sqlite://data/ladon.db"
max_connections = 1

[chains.ethereum-mainnet]
caip2 = "eip155:1"

[ladon]
store = "shared"

[ladon.derive.secret]
kind = "env"
var = "LADON_TEST_UNIVERSAL_SECRET"

[[ladon.derive.chains]]
name = "evm"
chain = "ethereum-mainnet"

[ladon.pool]
target = 10
threshold = 5
batch = 5
interval_secs = 1
"#,
        )
        .unwrap();
        let report = check(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        unsafe { std::env::remove_var("LADON_TEST_UNIVERSAL_SECRET") };
        assert_eq!(report.store_driver, "sqlite");
        assert!(!format!("{report:?}").contains("not-in-report"));
    }

    #[test]
    fn load_postgres_store_fails_without_feature() {
        if cfg!(feature = "postgres") || cfg!(feature = "pg") {
            return; // skip
        }
        let dir = std::env::temp_dir();
        let path = dir.join(format!("ladon-test-pg-fail-{}.toml", std::process::id()));

        let toml = r#"
            [stores.ladon]
            driver = "postgres"
            url = "postgres://localhost/ladon"

            [ladon]
            store = "ladon"

            [ladon.derive.secret]
            kind = "env"
            var = "LADON_MNEMONIC"
        "#;
        std::fs::write(&path, toml).unwrap();
        let result = load(&path);
        let _ = std::fs::remove_file(&path);
        assert!(result.is_err(), "Should reject postgres without feature");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("pg") || msg.contains("postgres"),
            "Error should mention pg/postgres feature: {msg}"
        );
    }

    #[test]
    fn load_postgres_store_succeeds_with_feature() {
        if !(cfg!(feature = "postgres") || cfg!(feature = "pg")) {
            return;
        }
        let dir = std::env::temp_dir();
        let path = dir.join(format!("ladon-test-pg-ok-{}.toml", std::process::id()));

        let toml = r#"
            [stores.ladon]
            driver = "postgres"
            url = "${LADON_TEST_DATABASE_URL}"

            [ladon]
            store = "ladon"

            [ladon.derive.secret]
            kind = "env"
            var = "LADON_MNEMONIC"

            [[ladon.derive.chains]]
            name = "evm"
            chain = "ethereum-mainnet"

            [ladon.pool]
            target = 10
            threshold = 5
            batch = 5
            interval_secs = 1

            [chains.ethereum-mainnet]
            caip2 = "eip155:1"
        "#;
        unsafe {
            std::env::set_var(
                "LADON_TEST_DATABASE_URL",
                "postgres://user:pass@localhost:5432/ladon",
            )
        };
        std::fs::write(&path, toml).unwrap();
        let result = load(&path);
        let _ = std::fs::remove_file(&path);
        unsafe { std::env::remove_var("LADON_TEST_DATABASE_URL") };
        assert!(
            result.is_ok(),
            "Should accept postgres with feature: {:?}",
            result.err()
        );
    }

    #[test]
    fn load_accepts_valid_shared_chain_reference() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!(
            "ladon-test-valid-chain-ref-{}.toml",
            std::process::id()
        ));

        let toml = r#"
            [chains.eth]
            caip2 = "eip155:1"

            [stores.ladon]
            driver = "sqlite"
            url = "sqlite://data/test.db"

            [ladon]
            store = "ladon"

            [ladon.derive.secret]
            kind = "env"
            var = "LADON_MNEMONIC"

            [[ladon.derive.chains]]
            name = "evm"
            chain = "eth"

            [ladon.pool]
            target = 10
            threshold = 5
            batch = 5
            interval_secs = 1
        "#;
        std::fs::write(&path, toml).unwrap();
        let cfg = load(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        assert_eq!(cfg.derive.chains[0].chain, "eth");
    }

    #[test]
    fn load_rejects_unknown_shared_chain_reference() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!(
            "ladon-test-bad-chain-ref-{}.toml",
            std::process::id()
        ));

        let toml = r#"
            [chains.eth]
            caip2 = "eip155:1"

            [stores.ladon]
            driver = "sqlite"
            url = "sqlite://data/test.db"

            [ladon]
            store = "ladon"

            [ladon.derive.secret]
            kind = "env"
            var = "LADON_MNEMONIC"

            [[ladon.derive.chains]]
            name = "evm"
            chain = "imaginary"

            [ladon.pool]
            target = 10
            threshold = 5
            batch = 5
            interval_secs = 1
        "#;
        std::fs::write(&path, toml).unwrap();
        let result = load(&path);
        let _ = std::fs::remove_file(&path);

        assert!(result.is_err(), "Should reject missing shared chain ref");
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("imaginary"), "{msg}");
        assert!(msg.contains("[chains.imaginary]"), "{msg}");
    }

    #[test]
    fn load_allows_empty_shared_chain_reference() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!(
            "ladon-test-empty-chain-ref-{}.toml",
            std::process::id()
        ));

        let toml = r#"
            [stores.ladon]
            driver = "sqlite"
            url = "sqlite://data/test.db"

            [ladon.derive.secret]
            kind = "env"
            var = "LADON_MNEMONIC"

            [[ladon.derive.chains]]
            name = "evm"
            chain = ""
        "#;
        std::fs::write(&path, toml).unwrap();
        let result = load(&path);
        let _ = std::fs::remove_file(&path);

        assert!(result.is_err(), "Shared chain references are required");
    }

    #[test]
    fn chain_start_index_defaults_to_zero() {
        let chain: ChainConfig = toml::from_str(
            r#"
            name = "evm"
            chain = "ethereum-mainnet"
            "#,
        )
        .unwrap();
        assert_eq!(chain.start_index, 0);
    }

    #[test]
    fn chain_start_index_deserializes_from_config() {
        let chain: ChainConfig = toml::from_str(
            r#"
            name = "solana"
            chain = "solana-mainnet"
            start_index = 1
            "#,
        )
        .unwrap();
        assert_eq!(chain.start_index, 1);
    }

    fn default_table() -> TableConfig {
        TableConfig {
            name: "derived_addresses".to_string(),
            columns: ColumnConfig {
                id: col_id(),
                chain: col_chain(),
                address: col_address(),
                path: col_path(),
                index: col_index(),
                is_used: col_is_used(),
                created_at: col_created_at(),
            },
        }
    }
}
