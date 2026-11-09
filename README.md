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

## Checks

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```
