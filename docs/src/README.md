# Introduction

`dns-manager` lets you declare DNS data once and render it into useful outputs:
BIND zonefiles, generic [octoDNS](https://github.com/octodns/octodns) config, and
Cloudflare-flavoured octoDNS config.

It is built for the case where DNS should come from the same source of truth as
the rest of your infrastructure, instead of being duplicated across Nix modules,
hand-written zone files, and provider-specific YAML.

## Architecture

DNS data is **declared in Nix** — through the `networking.domains` NixOS module
and/or a standalone `extraConfig` attribute set — and **resolved and rendered in
Rust**. The Nix layer is deliberately thin: it collects declarations and serializes
them to Pkl. The `dns-manager` binary does everything else: domain matching,
sub→base inheritance, multi-host merging, validation, and rendering.

```
NixOS module / extraConfig  ──serialize──▶  dns-manager  ──▶  zonefiles
        (declare)              (Pkl)         (resolve +        octoDNS config
                                              render)          cloudflare config
```

The binary is also a standalone CLI, so the same logic is usable and testable
outside Nix. See [Usage](usage.md) and the [CLI](cli.md) reference.
