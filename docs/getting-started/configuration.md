# Configuration

Ladon reads its namespace from a universal TOML file. Every local chain requires a matching shared `[chains.<id>]` entry. The selected `[stores.<id>]` is named by `ladon.store`.

```toml
[stores.ladon]
driver = "sqlite"
url = "sqlite://data/ladon/addresses.db"
[chains.ethereum-mainnet]
caip2 = "eip155:1"
[ladon]
store = "ladon"
[ladon.derive.secret]
kind = "env"
var = "LADON_MNEMONIC"
[[ladon.derive.chains]]
name = "evm"
chain = "ethereum-mainnet"
account = 0
change = 0
start_index = 0
[ladon.pool]
target = 1000
threshold = 200
batch = 100
interval_secs = 10
```

Secrets use `env`, `xpriv_env`, or `file`. PostgreSQL requires `url = "${DATABASE_URL}"`; inline credentials and defaults are rejected. Environment substitutions support `${NAME}` and `${NAME:-default}`, except PostgreSQL URLs which must be one required variable. `max_connections` is PostgreSQL-only (SQLite always uses one connection). Table and column names accept only ASCII SQL identifiers; this is the dynamic-SQL trust boundary.
