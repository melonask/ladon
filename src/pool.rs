use crate::{
    config::{ChainConfig, Config, SecretSource},
    db::{AddressRow, Db},
};
use anyhow::{Context, Result};
use chrono::Utc;
use ladon::{Params, canonical_chain, derive};
use tokio::time::{Duration, sleep};
use tracing::{error, info};
use ulid::Ulid;

/// Run the pool-daemon loop forever.
///
/// Periodically checks how many pre-generated addresses remain for each
/// configured chain. When the count falls below [`PoolConfig::threshold`],
/// new addresses are derived and inserted until the pool reaches
/// [`PoolConfig::target`].
pub async fn run(cfg: &Config, db: &Db) -> Result<()> {
    let interval = Duration::from_secs(cfg.pool.interval_secs);

    info!(
        target = cfg.pool.target,
        threshold = cfg.pool.threshold,
        batch = cfg.pool.batch,
        interval_secs = cfg.pool.interval_secs,
        "Pool daemon started",
    );

    loop {
        for chain_cfg in &cfg.derive.chains {
            if let Err(e) = tick(cfg, db, chain_cfg).await {
                error!(chain = %chain_cfg.name, error = %e, "Pool tick failed");
            }
        }
        sleep(interval).await;
    }
}

async fn tick(cfg: &Config, db: &Db, chain_cfg: &ChainConfig) -> Result<()> {
    let chain = canonical_chain(&chain_cfg.name)?;
    let count = db.count(&chain).await?;

    if count >= cfg.pool.threshold as i64 {
        return Ok(());
    }

    let needed = (cfg.pool.target as i64 - count).max(0) as u32;
    let batches = needed.div_ceil(cfg.pool.batch);
    info!(chain = %chain, pool = count, target = cfg.pool.target, "Refilling pool");

    let next_index = next_index(db.max_index(&chain).await?, chain_cfg.start_index);
    let (mnemonic, passphrase, xpriv) = resolve_secret(&cfg.derive.secret)?;

    let mut global_idx = next_index;
    for batch_idx in 0..batches {
        let remaining = needed - (batch_idx * cfg.pool.batch);
        let batch_size = remaining.min(cfg.pool.batch);
        let indexes: Vec<u32> = (global_idx..global_idx + batch_size).collect();
        let index_str = indexes
            .iter()
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(",");

        let wallet = derive(Params {
            chain: chain.clone(),
            mnemonic: mnemonic.clone(),
            passphrase: passphrase.clone().unwrap_or_default(),
            indexes: Some(index_str),
            account: chain_cfg.account,
            change: chain_cfg.change,
            network: chain_cfg.network.clone(),
            solana_mode: chain_cfg.solana_mode.clone(),
            program_id: chain_cfg.program_id.clone(),
            xpriv: xpriv.clone(),
            strength: 12,
            num: batch_size,
            ..Default::default()
        })?;

        let rows: Vec<AddressRow> = wallet
            .keys
            .iter()
            .map(|k| AddressRow {
                id: Ulid::new().to_string(),
                chain: chain.clone(),
                address: k.address.clone(),
                path: k.path.clone(),
                index: k.index,
                created_at: Utc::now().to_rfc3339(),
            })
            .collect();

        let inserted = db.insert(&rows).await?;
        global_idx += batch_size;
        info!(chain = %chain, inserted, "Batch inserted");
    }

    Ok(())
}

fn next_index(max_index: Option<u32>, start_index: u32) -> u32 {
    max_index.map(|i| i + 1).unwrap_or(start_index)
}

/// Resolve mnemonic / passphrase / xpriv from the configured [`SecretSource`].
fn resolve_secret(src: &SecretSource) -> Result<(Option<String>, Option<String>, Option<String>)> {
    match src {
        SecretSource::Env {
            var,
            passphrase_var,
        } => {
            let mnemonic =
                std::env::var(var).with_context(|| format!("Env var `{var}` not set"))?;
            let passphrase = passphrase_var
                .as_ref()
                .map(|v| {
                    std::env::var(v).with_context(|| format!("Passphrase env var `{v}` not set"))
                })
                .transpose()?;
            Ok((Some(mnemonic), passphrase, None))
        }
        SecretSource::XprivEnv { var } => {
            let xpriv = std::env::var(var).with_context(|| format!("Env var `{var}` not set"))?;
            Ok((None, None, Some(xpriv)))
        }
        SecretSource::File {
            path,
            passphrase_var,
        } => {
            let mnemonic = std::fs::read_to_string(path)
                .with_context(|| format!("Failed to read mnemonic file: {path}"))?;
            let mnemonic = mnemonic.trim().to_string();
            let passphrase = passphrase_var
                .as_ref()
                .map(|v| {
                    std::env::var(v).with_context(|| format!("Passphrase env var `{v}` not set"))
                })
                .transpose()?;
            Ok((Some(mnemonic), passphrase, None))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::next_index;

    #[test]
    fn next_index_uses_start_index_when_chain_has_no_rows() {
        assert_eq!(next_index(None, 1), 1);
    }

    #[test]
    fn next_index_continues_after_existing_max_index() {
        assert_eq!(next_index(Some(7), 1), 8);
    }
}
