# stb-web

The website for **SistemTokoBangunan** (sistemtokobangunan.com): server-rendered pages served by
one Rust binary built on axum and tokio, with SQLite storage. See RFC-0001, ADR-0003 and ADR-0009
in the SistemTokoBangunan wiki.

## Run

```sh
STB_DATABASE_PATH=./dev.db \
STB_BASE_URL=http://127.0.0.1:8080 \
STB_WHATSAPP_NUMBER=6281234567890 \
cargo run -- serve
```

Then open <http://127.0.0.1:8080/healthz>, which answers `ok`.

## Environment variables

| Variable | Required | Meaning |
|---|---|---|
| `STB_DATABASE_PATH` | yes | Path of the SQLite database file. |
| `STB_BASE_URL` | yes | Public base URL of the site, e.g. `https://sistemtokobangunan.com`. A trailing `/` is removed. |
| `STB_WHATSAPP_NUMBER` | yes | The store owner's WhatsApp number used in contact links, e.g. `6281234567890`. |
| `STB_BIND` | no | Address and port to listen on. Default `127.0.0.1:8080`. |
| `RUST_LOG` | no | Log filter for `tracing-subscriber`. Default `info`. |

## Owner commands

Both need only `STB_DATABASE_PATH`. They print one line (a count and a file name, never lead
data) and exit 0; errors go to stderr with exit 1; wrong arguments print the usage with exit 2.

```sh
STB_DATABASE_PATH=./dev.db cargo run -- waitlist-export waitlist.csv   # UTF-8 CSV for Excel
STB_DATABASE_PATH=./dev.db cargo run -- backup backup-2026-11-09.db     # refuses an existing file
```

The CSV holds every waitlist sign-up, oldest first. Values starting with `=`, `+`, `-` or `@`
(including WhatsApp numbers, `'+628…`) get a leading `'` so a spreadsheet does not run them as
formulas. The backup is a consistent copy made with SQLite's online backup API, safe while the
server runs.

## Checks

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```
