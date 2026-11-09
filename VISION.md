# dns-manager vision

`dns-manager` lets infrastructure declare DNS data once, next to the rest of
its configuration, and renders that declaration into BIND zonefiles, generic
octoDNS configuration, Cloudflare-flavoured octoDNS configuration, and Caddy
routes. It is used by flakes that already describe hosts and services in Nix,
and it is also a standalone CLI so the same resolution logic can be exercised
and tested outside Nix.

## Problem

DNS records drift when they are duplicated across Nix modules, hand-written zone
files, and provider-specific YAML. The hardest case is a name computed from
other configuration — a module setting
`domain = "${config.networking.hostName}.${config.networking.domain}"` — because
the rendered records must follow that declaration rather than a copied string.

## Principles

- Declare in Nix, resolve and render in Rust. The Nix layer only collects
  declarations and serializes them to Pkl; the binary owns domain matching,
  sub-to-base inheritance, multi-host merging, validation, and rendering.
- Keep the Nix layer thin so the same logic is testable outside Nix and usable
  as a plain CLI.
- Report every violation at once at build time instead of failing module
  evaluation incrementally.
- Keep the serialized contract explicit: `pkl/DnsConfig.pkl` is the standalone
  Pkl contract for external CLI configuration.
- Treat providers as render targets, not as the source of truth; the flake
  exposes `zonefiles`, `octodns`, `cloudflare`, `caddyRoutes`, and `resolve`
  derivations and helpers.

## Scope

The `networking.domains` NixOS module and a standalone `extraConfig` attrset,
the collector that turns declarations into Pkl, resolution and validation, the
zonefile/octoDNS/Cloudflare/Caddy renderers, the `dns-manager` CLI
(`resolve`/`zonefile`/`octodns`/`cloudflare`), a worked multi-host example, and
mdBook documentation.

## Non-goals

It is not a DNS server and not an octoDNS runner: it produces the configuration
those tools consume. Provider API access, zone transfer, and authoritative
serving stay with the deployed infrastructure.

## Direction

The project is in the middle of a deliberate restructuring: all DNS logic moved
from Nix into the Rust binary and the public flake API was redesigned
(`utils.generate` to `lib.generate`, `utils.debug.*` to `lib.collect`), which
the changelog still records as unreleased. Recent commits after that are
dependency and CI maintenance.

## Place in the fleet

Canix consumes this flake directly and renders provider configuration only
during the Atlas-side Nix build; `thething` is explicitly not allowed to run
octoDNS services or own an octoDNS user. It is also one of the gitlinked
components qualified by the `fleet` superproject.
