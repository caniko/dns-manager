# Contributing / Development

Thank you for considering a contribution. Small fixes can go straight to a PR;
for larger changes (e.g. a new provider backend) please open an issue or a draft
PR first so we can discuss the approach.

## Layout

- `crates/dns-manager/` — the Rust library + CLI that does all resolving and
  rendering. Module map: `resolve/` (domain matching, merge, inheritance,
  normalization), `validate.rs`, `render/` (zonefile, octodns, cloudflare, yaml).
- `nix/` — the thin Nix layer: the `networking.domains` module, `collect.nix`
  (the raw contract producer), `generate.nix` (Pkl render derivations), and the
  crane build wiring.
- `docs/book/` — mdBook sources; `module-options.md` is generated from the module.

## Toolchain

Everything is in the dev shell — run `nix develop` or `direnv allow`:

| Tooling                             | Usage                                          |
| ----------------------------------- | ---------------------------------------------- |
| cargo / rustc (rs-harbor toolchain) | `cargo build`, `cargo run -p dns-manager -- …` |
| cargo-nextest                       | `cargo nextest run`                            |
| treefmt (Nix/Markdown/YAML)         | `nix fmt`                                      |
| bind / octodns                      | `named-checkzone`, `octodns-sync`              |

## Checks

The flake `checks` mirror CI:

```bash
cargo nextest run                              # or: nix build .#checks.<system>.nextest
cargo clippy --all-targets -- --deny warnings  # nix build .#checks.<system>.clippy
cargo fmt --check                              # nix build .#checks.<system>.fmt
nix flake check
```

When you change behaviour, update or add a test under `crates/dns-manager/tests/`
(the `resolve`/`zonefile`/`render` goldens are the regression oracles) and note
breaking changes in `CHANGELOG.md`.

## Docs

```bash
nix build .#docs && xdg-open result/index.html
```

`module-options.md` is generated from `nix/module.nix` via `nixosOptionsDoc`; the
rest of `docs/book/` is plain mdBook.

When adding examples, use the reserved resources:

| Resource | RFC                                                       |
| -------- | --------------------------------------------------------- |
| Domains  | [RFC 2606](https://datatracker.ietf.org/doc/html/rfc2606) |
| IPv6     | [RFC 3849](https://datatracker.ietf.org/doc/html/rfc3849) |
| IPv4     | [RFC 5737](https://datatracker.ietf.org/doc/html/rfc5737) |
