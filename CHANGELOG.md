# Changelog

## Unreleased

### Changed (breaking)

- **Rewrite: all DNS logic moved from Nix into a Rust binary (`dns-manager`).**
  Domain matching, sub→base inheritance, multi-host merge, validation, and
  rendering (zonefiles, octoDNS, Cloudflare) are now done in Rust. The Nix layer
  only declares data and serializes it to JSON.
- **Public flake API redesigned.** `utils.generate` → `lib.generate` with
  `{ zonefiles, octodns, cloudflare, resolve }`; `utils.debug.*` → `lib.collect`
  (raw) and `lib.generate.resolve` (resolved). See [MIGRATION.md](MIGRATION.md).
- Repository reorganized: Rust crate under `crates/dns-manager/`, thin Nix layer
  under `nix/`. The `networking.domains` module is now a pure declaration with no
  inheritance/validation logic.
- The flake builds with crane (via rs-harbor); inputs migrated accordingly.
- Validation now reports all violations at once at build time rather than failing
  module evaluation incrementally.

### Added

- `packages.dns-manager` — the standalone CLI (`resolve`/`zonefile`/`octodns`/
  `cloudflare`).
- Rust test suite porting the legacy nix-unit goldens.

### Removed

- The pure-Nix `utils/` and `modules/` trees, the checked-in `.mypy_cache`, the
  empty `helper.nix`/`types.nix` stubs, and the nix-unit `tests` package.
