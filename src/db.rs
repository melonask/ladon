use crate::config::{ColumnConfig, DbConfig};
use anyhow::{Context, Result};
use sqlx::{AnyPool, AssertSqlSafe, Row, any::AnyPoolOptions};
use tracing::info;

/// A thin handle around an [`AnyPool`] plus the resolved table/column names.
pub struct Db {
    pool: AnyPool,
    table: String,
    cols: ColumnConfig,
}

impl Db {
    /// Open a connection pool from `cfg`.
    pub async fn connect(cfg: &DbConfig) -> Result<Self> {
        sqlx::any::install_default_drivers();

        if let Some(sqlite) = &cfg.sqlite {
            let url = format!("sqlite://{}?mode=rwc", sqlite.path);
            let pool = AnyPoolOptions::new()
                .max_connections(1)
                .connect(&url)
                .await
                .context("SQLite connection failed")?;
            info!(path = %sqlite.path, "Connected to SQLite");
            return Ok(Self {
                pool,
                table: sqlite.table.name.clone(),
                cols: sqlite.table.columns.clone(),
            });
        }

        if let Some(pg) = &cfg.postgres {
            let url = pg.url()?;
            let pool = AnyPoolOptions::new()
                .max_connections(pg.pool_size)
                .connect(&url)
                .await
                .context("Postgres connection failed")?;
            info!("Connected to Postgres");
            return Ok(Self {
                pool,
                table: pg.table.name.clone(),
                cols: pg.table.columns.clone(),
            });
        }

        anyhow::bail!("No database backend configured");
    }

    /// Create the address table if it doesn't already exist.
    pub async fn ensure_schema(&self) -> Result<()> {
        let sql = format!(
            r#"
            CREATE TABLE IF NOT EXISTS "{table}" (
                "{id}"         TEXT PRIMARY KEY,
                "{chain}"      TEXT NOT NULL,
                "{address}"    TEXT NOT NULL,
                "{path}"       TEXT NOT NULL,
                "{index}"      INTEGER NOT NULL,
                "{is_used}"    BOOLEAN NULL,
                "{created_at}" TEXT NOT NULL
            )
            "#,
            table = self.table,
            id = self.cols.id,
            chain = self.cols.chain,
            address = self.cols.address,
            path = self.cols.path,
            index = self.cols.index,
            is_used = self.cols.is_used,
            created_at = self.cols.created_at,
        );
        // Table and column names are resolved from Ladon's strict config schema;
        // value inputs remain bound parameters. SQLx 0.9 requires an explicit
        // audit marker for dynamic identifier SQL.
        sqlx::query(AssertSqlSafe(sql))
            .execute(&self.pool)
            .await
            .context("Schema creation failed")?;
        self.ensure_is_used_column().await?;
        Ok(())
    }

    async fn ensure_is_used_column(&self) -> Result<()> {
        let sql = format!(
            r#"ALTER TABLE "{}" ADD COLUMN "{}" BOOLEAN NULL"#,
            self.table, self.cols.is_used,
        );
        match sqlx::query(AssertSqlSafe(sql)).execute(&self.pool).await {
            Ok(_) => Ok(()),
            Err(e) if is_duplicate_column_error(&e) => Ok(()),
            Err(e) => Err(e).context("is_used column migration failed"),
        }
    }

    /// Count available addresses in the pool for `chain`.
    pub async fn count(&self, chain: &str) -> Result<i64> {
        let sql = format!(
            r#"SELECT COUNT(*) as cnt FROM "{}" WHERE "{}" = $1 AND "{}" IS NULL"#,
            self.table, self.cols.chain, self.cols.is_used,
        );
        let row = sqlx::query(AssertSqlSafe(sql))
            .bind(chain)
            .fetch_one(&self.pool)
            .await
            .context("count query failed")?;
        Ok(row.try_get::<i64, _>("cnt").unwrap_or(0))
    }

    /// Insert a batch of addresses for `chain`.
    pub async fn insert(&self, rows: &[AddressRow]) -> Result<u64> {
        if rows.is_empty() {
            return Ok(0);
        }

        let mut tx = self.pool.begin().await.context("begin transaction")?;
        let sql = format!(
            r#"INSERT INTO "{table}" ("{id}", "{chain}", "{address}", "{path}", "{index}", "{created_at}")
               VALUES ($1, $2, $3, $4, $5, $6)"#,
            table = self.table,
            id = self.cols.id,
            chain = self.cols.chain,
            address = self.cols.address,
            path = self.cols.path,
            index = self.cols.index,
            created_at = self.cols.created_at,
        );

        for row in rows {
            sqlx::query(AssertSqlSafe(sql.clone()))
                .bind(&row.id)
                .bind(&row.chain)
                .bind(&row.address)
                .bind(&row.path)
                .bind(row.index as i64)
                .bind(&row.created_at)
                .execute(&mut *tx)
                .await
                .context("insert failed")?;
        }

        tx.commit().await.context("commit failed")?;
        Ok(rows.len() as u64)
    }

    /// Return the highest derivation index stored for `chain`, or `None`.
    pub async fn max_index(&self, chain: &str) -> Result<Option<u32>> {
        let sql = format!(
            r#"SELECT MAX("{}") as mx FROM "{}" WHERE "{}" = $1"#,
            self.cols.index, self.table, self.cols.chain,
        );
        let row = sqlx::query(AssertSqlSafe(sql))
            .bind(chain)
            .fetch_optional(&self.pool)
            .await
            .context("max_index query failed")?;
        Ok(row
            .and_then(|r| r.try_get::<i64, _>("mx").ok())
            .map(|v| v as u32))
    }
}

fn is_duplicate_column_error(e: &sqlx::Error) -> bool {
    let msg = e.to_string().to_lowercase();
    msg.contains("duplicate column") || msg.contains("already exists")
}

/// A single address row ready for insertion.
#[derive(Debug)]
pub struct AddressRow {
    pub id: String,
    pub chain: String,
    pub address: String,
    pub path: String,
    pub index: u32,
    pub created_at: String,
}
