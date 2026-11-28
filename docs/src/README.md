# Introduction

Declare DNS data once with `dns-manager` and render BIND zonefiles, generic
[octoDNS](https://github.com/octodns/octodns) config, and Cloudflare-flavoured
octoDNS config.

Use `dns-manager` when you want DNS to share a source of truth with the rest of
your infrastructure, instead of duplicating it across Nix modules, hand-written
zone files, and provider-specific YAML.

## Architecture

DNS data is **declared in Nix** — through the `networking.domains` NixOS module
and/or a standalone `extraConfig` attribute set — and **resolved and rendered in
Rust**. The Nix layer is deliberately thin: it collects declarations and
serializes them to Pkl. The `dns-manager` binary handles the rest: domain
matching, sub→base inheritance, multi-host merging, validation, and rendering.

```
NixOS module / extraConfig  ──serialize──▶  dns-manager  ──▶  zonefiles
        (declare)              (Pkl)         (resolve +        octoDNS config
                                              render)          cloudflare config
```

The binary is also a standalone CLI, so the same logic is usable and testable
outside Nix. See [Usage](usage.md) and the [CLI](cli.md) reference.
