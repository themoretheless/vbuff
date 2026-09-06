# Local storage benchmark — 2026-09-06

Baseline: `70a877d`, plus the benchmark example added with this report. Apple M4 Max, 128 GiB RAM, macOS 26.7, arm64, Rust 1.97.0. Release builds; compilation excluded. Engines ran sequentially on the same machine. No CPU isolation or cold filesystem cache guarantee.

## Method

Each backend uses a fresh temporary on-disk database for 1,000 and 10,000 unique synthetic text clips, approximately 0.5 KiB each, containing English and Russian text. Clip generation and hashing are outside insertion timing. No user clipboard data is read. Each corpus has one insertion pass; this is one process run per backend, not a multi-run statistical study.

Insert samples cover the growing database; read samples cover the final size. Repeated insertion targets the same existing clip 100 times. Reads/searches have one untimed warm-up; each reported read/search has 100 timed samples. The common term matches 10% of the corpus, returning 20 clips; the missing term matches none. Recall measures one unfiltered 64-item page, not the full history search worker or layout-aware ranking. Reopen has 10 samples and includes close/drop. Assertions check result sizes. Percentiles use nearest rank and include Rust result allocation, validation and destruction.

## Results

All operation times below are milliseconds: **p50 / p95**.

### 1,000 clips

| Operation | DuckDB | SQLite |
|---|---:|---:|
| insert | 1.222 / 1.476 | 0.409 / 0.646 |
| deduplicate | 1.426 / 1.669 | 0.507 / 0.730 |
| list_50 | 0.746 / 0.847 | 1.705 / 2.016 |
| search_common | 2.115 / 2.301 | 8.556 / 8.829 |
| search_missing | 2.153 / 2.283 | 0.535 / 0.704 |
| recall_page_64 | 1.161 / 1.508 | 0.386 / 0.460 |
| reopen | 14.295 / 15.575 | 6.150 / 6.451 |

### 10,000 clips

| Operation | DuckDB | SQLite |
|---|---:|---:|
| insert | 1.307 / 1.721 | 0.388 / 0.613 |
| deduplicate | 1.998 / 2.283 | 0.980 / 1.293 |
| list_50 | 3.515 / 3.717 | 13.950 / 14.589 |
| search_common | 16.488 / 17.173 | 873.779 / 886.795 |
| search_missing | 18.030 / 18.391 | 6.101 / 6.494 |
| recall_page_64 | 4.916 / 5.151 | 0.392 / 0.441 |
| reopen | 23.158 / 27.243 | 51.449 / 52.945 |

### Total insertion time and closed database footprint

| Corpus | DuckDB insert | SQLite insert | DuckDB files | SQLite files |
|---|---:|---:|---:|---:|
| 1,000 | 1.272 s | 0.476 s | 6.26 MiB | 5.21 MiB |
| 10,000 | 13.838 s | 4.986 s | 29.26 MiB | 49.67 MiB |

File sizes are the total of regular files in each temporary database directory after closing the database, including sidecars if present; not peak disk usage.

Core capture hashing + classification: **184 ms for 10,000 × 16 KiB** iterations (about 18.4 µs/iteration). This existing budget also includes allocating/copying the flavor bytes; it is not a standalone BLAKE3 throughput measurement. The 3,000 ms budget passed.

## Findings and next optimization targets

- The SQLite common-match search is the clearest hotspot: p50 rises from 8.556 ms at 1k to 873.779 ms at 10k (~102×). DuckDB rises from 2.115 to 16.488 ms (~7.8×). At 10k, this SQLite path is ~53× slower. Inspect `search_page` using EXPLAIN QUERY PLAN before changing its FTS membership subquery, joins or ordering. The benchmark establishes latency, not the root cause.
- SQLite list latency rises ~8.2× with a 10× corpus increase. Inspect filtering and the pinned/updated/sequence ordering; verify a covering order index can be used before prescribing one.
- SQLite insertion is ~2.8× faster by total elapsed time at 10k, and the 64-item recall page is ~12.5× faster by p50. Therefore these results do not justify declaring either engine universally faster.
- Actual popup responsiveness, full-history ranking, image/CAS workloads, memory consumption, concurrent capture and cold startup remain unmeasured. No production query or backend selection was changed by this benchmark.

## Reproduce

From the repository root, run sequentially:

```sh
cargo run --release -p vbuff-store --example storage_benchmark --locked > duckdb.csv 2> duckdb.log
cargo run --release -p vbuff-store --example storage_benchmark --no-default-features --features sqlite --locked > sqlite.csv 2> sqlite.log
cargo test --release -p vbuff-core --test performance_budget --locked -- --ignored --nocapture
```

Raw results: [DuckDB CSV](duckdb.csv), [SQLite CSV](sqlite.csv), [DuckDB log](duckdb.log), [SQLite log](sqlite.log), [core log](core.log).
Benchmark source: [storage_benchmark.rs](../../../crates/vbuff-store/examples/storage_benchmark.rs).
