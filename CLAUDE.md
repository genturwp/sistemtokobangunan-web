# stb-web — sistemtokobangunan.com

The website and (later) licence service of **SistemTokoBangunan**: one Rust binary (axum on
tokio, server-rendered templates, SQLite), decided in ADR-0003 and ADR-0009 of the wiki and
proposed in RFC-0001.

Read first:

- `../CLAUDE.md` — rules shared by every SistemTokoBangunan code repo.
- The wiki: `/Users/genturwidyaputra/Documents/projects/sistemtokobangunan/sistemtokobangunan-wiki`
  (`docs/index.md`, `docs/rfcs/0001-distribution-website.md`, `docs/decisions/0003-*`,
  `docs/decisions/0009-*`). Plans for this repo live in the wiki's `plans/`.

## Checks

All three pass before every commit:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## Rules

- Visitor-facing text is Bahasa Indonesia; code, comments and logs are English.
- No secrets in the repo: configuration comes from `STB_*` environment variables.
- Never write lead names, numbers or emails to logs.
- Crates only from ADR-0003 / ADR-0009; a new crate needs an ADR in the wiki first.
- Work on a feature branch; never commit to `main` directly.
