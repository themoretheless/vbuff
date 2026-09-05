# Storage backends

vbuff supports DuckDB and SQLite through the same public `Store` API and single owning history worker. The engine is selected when compiling, not in application settings. Enabling both engine features is rejected to prevent ambiguous builds.

| Build | Command | History file | Payload directory |
| --- | --- | --- | --- |
| DuckDB (default) | `cargo build --release` | `history.duckdb` | `duckdb-blobs/` |
| SQLite | `cargo build --release --no-default-features --features sqlite,tray` | `history.db` | `blobs/` |

For builds without the tray, use `--no-default-features --features duckdb` or `--no-default-features --features sqlite`. With no features at all, the store falls back to SQLite. SQLite builds do not compile DuckDB or require its C++ build.

Both engines provide persistence, lifecycle operations, exports, full-history popup recall, and the CLI. The SQLite implementation retains FTS5 and its schema migrations; DuckDB uses native projection scans. Low-level search planning and maintenance are engine-specific. `vbuff doctor` reports the engine of the responding resident process, including when the CLI was built for another engine.

## Switching builds

Quit the resident application before starting a build with another engine. The two builds use the same single-instance endpoint. Their database files and payload directories remain separate.

A first DuckDB startup can import the existing SQLite history if `history.duckdb` does not exist. The import validates the source and copied data before publishing the new database and retains the SQLite source. See [migration details](duckdb-migration.md).

There is no automatic reverse migration or bidirectional synchronization. Starting SQLite after using DuckDB opens the existing SQLite history, which does not include later DuckDB changes. An existing DuckDB database is not reimported every time SQLite changes.

Neither backend currently provides database encryption or Touch ID unlocking.

## Validation

Run the application and store suites for each backend separately:

```sh
cargo test -p vbuff -p vbuff-store --no-default-features --features duckdb
cargo test -p vbuff -p vbuff-store --no-default-features --features sqlite
```

CI tests both backends on each desktop platform. These local tests do not establish live desktop behavior on other operating systems.
