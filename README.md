# Deadpool for CitadelDB

Deadpool is a dead simple async pool for connections and objects
of any type.

This crate implements a [`deadpool`](https://crates.io/crates/deadpool)
manager for [`citadeldb`](https://crates.io/crates/citadeldb)
and provides async connection pooling via the blocking thread pool.

## Features

| Feature          | Description                                                              | Extra dependencies               | Default |
| ---------------- | ------------------------------------------------------------------------ | -------------------------------- | ------- |
| `rt_tokio_1`     | Enable support for [tokio](https://crates.io/crates/tokio) crate         | `deadpool/rt_tokio_1`            | yes     |
| `rt_async-std_1` | Enable support for [async-std](https://crates.io/crates/async-std) crate | `deadpool/rt_async-std_1`        | no      |
| `serde`          | Enable support for [serde](https://crates.io/crates/serde) crate         | `deadpool/serde`, `serde/derive` | no      |

## Example

```rust
use deadpool_citadeldb::{Config, Runtime};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = Config::new("", b"secret");
    let pool = cfg.create_pool(Runtime::Tokio1)?;
    let conn = pool.get().await?;
    let result: i64 = conn
        .interact(|inner| {
            inner.execute("CREATE TABLE IF NOT EXISTS t (id INTEGER PRIMARY KEY, name TEXT)")
                .expect("Failed to create table");
            let result = inner.query("SELECT 1")
                .expect("Failed to query");
            let row = result.rows.first().expect("No rows returned");
            let val = row.first().expect("Empty row");
            match val {
                citadel_sql::Value::Integer(i) => *i,
                _ => panic!("Expected integer"),
            }
        })
        .await?;
    assert_eq!(result, 1);
    Ok(())
}
```

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
