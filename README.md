# dns-manager

<!-- simit:badges:start -->

[![CI](https://img.shields.io/badge/CI-managed-2088ff)](.github/workflows/ci.yaml) [![Nix](https://img.shields.io/badge/Nix-drift-5277c3)](flake.nix) [![docs](https://img.shields.io/badge/docs-enabled-6f42c1)](docs) [![crates.io](https://img.shields.io/badge/crates.io-publishable-f46623)](https://crates.io/crates/dns-manager)

<!-- simit:badges:end -->

`dns-manager` declares DNS data once and renders BIND zonefiles, generic
[octoDNS](https://github.com/octodns/octodns) config, and Cloudflare-flavoured
octoDNS config.

Use `dns-manager` when you want DNS to share a source of truth with the rest of
your infrastructure, instead of duplicating it across Nix modules, hand-written
zone files, and provider-specific YAML.

## How it works

DNS data is **declared in Nix** and **resolved and rendered in Rust**.

- The Nix layer is thin: declare data through the `networking.domains` NixOS
  module and a standalone `extraConfig` attrset; `dns-manager.lib.collect`
  serializes it to Pkl for the binary.
- The `dns-manager` Rust binary handles the rest — domain matching, sub→base
  inheritance, multi-host merging, validation, and rendering. It also runs as a
  standalone CLI, usable and testable outside Nix.

```
NixOS module / extraConfig  ──collect──▶  dns-manager  ──▶  zonefiles
        (declare)            (Pkl)         (resolve +        octoDNS config
                                            render)          cloudflare config
```

This is especially useful when names are computed rather than static: a module
can set `domain = "${config.networking.hostName}.${config.networking.domain}"`
and the DNS outputs follow from that declaration directly.

## Flake outputs

- `nixosModules.dns` — the `networking.domains` module.
- `lib.generate pkgs` → `{ zonefiles, octodns, cloudflare, caddyRoutes, resolve }`
  render derivations/helpers.
- `lib.collect dnsConfig` — pure-Nix view of the collected raw config.
- `packages.dns-manager` — the CLI binary; `apps`/`packages.default` run it.
- `pkl/DnsConfig.pkl` — the standalone Pkl contract for external CLI configs.
- `packages.docs` — the mdBook documentation.

## Quick start

```bash
nix flake init -t git+https://github.com/caniko/dns-manager
```

Or read [example/flake.nix](example/flake.nix), [example/dns.nix](example/dns.nix),
and the standalone Pkl examples [example/dns.pkl](example/dns.pkl) /
[example/cloudflare.pkl](example/cloudflare.pkl).

A minimal renderer call:

```nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    dns-manager.url = "git+https://github.com/caniko/dns-manager";
    dns-manager.inputs.nixpkgs.follows = "nixpkgs";
  };
  outputs = { self, nixpkgs, dns-manager, ... }: let
    system = "x86_64-linux";
    dnsConfig = { inherit (self) nixosConfigurations; extraConfig = import ./dns.nix; };
    generate = dns-manager.lib.generate nixpkgs.legacyPackages.${system};
  in {
    packages.${system}.cloudflare = generate.cloudflare {
      inherit dnsConfig;
      token = { type = "env"; name = "CLOUDFLARE_API_TOKEN"; };
      zones."example.com".mode = "lenient";
    };
  };
}
```

See the [documentation](docs/src/usage.md) for the full guide, and
[MIGRATION.md](MIGRATION.md) if you used a pre-Rust version.

## Cloudflare cache purge

`cloudflare-purge` purges files from Cloudflare's edge cache for one zone,
reading the API token from a file so it never appears in process arguments.

The token needs **Zone:Read** (to resolve the zone name to its id) and
**Zone:Cache Purge** for that zone; one zone-scoped API token can hold both.

```bash
# Raw token, or env-style lines such as CLOUDFLARE_API_TOKEN="..."
install -m 600 /dev/null /run/secrets/cloudflare-token
printf '%s' "$CLOUDFLARE_API_TOKEN" > /run/secrets/cloudflare-token

dns-manager cloudflare-purge \
  --zone example.com \
  --token-file /run/secrets/cloudflare-token \
  --url https://example.com/ \
  --url https://www.example.com/app.js
```

Each URL must be an absolute `http(s)` URL whose host is the zone itself or one
of its subdomains, with 1 to 30 URLs per call (Cloudflare's per-request limit).
`--api-base` overrides the Cloudflare API endpoint. On success the command
prints a JSON summary:

```json
{ "zone": "example.com", "zone_id": "…", "purged": ["https://example.com/"] }
```

## Development

```bash
nix develop          # cargo, nextest, bind, octodns, mdbook
cargo nextest run
nix flake check
nix build .#docs && xdg-open result/index.html
```

## License

MIT. See [LICENSE](LICENSE).
