# dns-manager

`dns-manager` declares DNS data once and renders it into useful outputs: BIND
zonefiles, generic [octoDNS](https://github.com/octodns/octodns) config, and
Cloudflare-flavoured octoDNS config.

It is built for the case where DNS should come from the same source of truth as
the rest of your infrastructure, instead of being duplicated across Nix modules,
hand-written zone files, and provider-specific YAML.

## How it works

DNS data is **declared in Nix** and **resolved and rendered in Rust**.

- The Nix layer is thin: the `networking.domains` NixOS module and a standalone
  `extraConfig` attrset let you declare data; `dns-manager.lib.collect`
  serializes it to JSON.
- The `dns-manager` Rust binary does everything else — domain matching, sub→base
  inheritance, multi-host merging, validation, and rendering. It is also a
  standalone CLI, usable and testable outside Nix.

```
NixOS module / extraConfig  ──collect──▶  dns-manager  ──▶  zonefiles
        (declare)            (JSON)        (resolve +        octoDNS config
                                            render)          cloudflare config
```

That matters most when names are computed rather than static — a module can set
`domain = "${config.networking.hostName}.${config.networking.domain}"` and the
DNS outputs follow from that declaration directly.

## Flake outputs

- `nixosModules.dns` — the `networking.domains` module.
- `lib.generate pkgs` → `{ zonefiles, octodns, cloudflare, resolve }` render
  derivations.
- `lib.collect dnsConfig` — pure-Nix view of the collected raw config.
- `packages.dns-manager` — the CLI binary; `apps`/`packages.default` run it.
- `packages.docs` — the mdBook documentation.

## Quick start

```bash
nix flake init -t git+https://codeberg.org/caniko/dns-manager
```

Or read [example/flake.nix](example/flake.nix) and [example/dns.nix](example/dns.nix).

A minimal renderer call:

```nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    dns-manager.url = "git+https://codeberg.org/caniko/dns-manager";
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

See the [documentation](docs/book/usage.md) for the full guide, and
[MIGRATION.md](MIGRATION.md) if you used a pre-Rust version.

## Development

```bash
nix develop          # cargo, nextest, bind, octodns, mdbook
cargo nextest run
nix flake check
nix build .#docs && xdg-open result/index.html
```

## License

MIT. See [LICENSE](LICENSE).
