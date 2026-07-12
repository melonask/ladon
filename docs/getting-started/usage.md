# Usage

```sh
ladon --config /etc/stack/Config.toml check
ladon --config /etc/stack/Config.toml pool
```

`check` success output:

```text
configuration valid: store_driver=sqlite, table=derived_addresses, chains=2, pool_target=1000
```

`pool` creates its table when needed and refills each chain when available rows (`is_used IS NULL`) fall below `threshold`. It continues at the highest persisted index plus one. Consumers should atomically claim rows and set `is_used = true` to preserve index continuity. Logs use stderr; validation failures exit non-zero without disclosing secret values.
