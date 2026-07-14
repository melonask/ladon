# Operations and responses

## `check`

Validates TOML shape, store selection, chain references, pool bounds, SQL identifiers, and secret-reference availability. Success prints one non-secret line; failure writes an actionable error to stderr and exits non-zero.

## `pool`

Runs until stopped. Each successful refill emits structured stderr logs similar to the illustrative output below; timestamps, module prefixes, and other tracing fields vary with configuration:

```text
INFO Pool daemon started target=1000 threshold=200 batch=100 interval_secs=10
INFO Refilling pool chain=evm pool=0 target=1000
INFO Batch inserted chain=evm inserted=100
```

One chain's failure is logged and retried at the next interval. Connection and schema failures before the loop terminate the command. The CLI never emits addresses or private keys.
