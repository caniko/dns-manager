# Debugging

## Inspect the collected (raw) config

`dns-manager.lib.collect dnsConfig` is pure Nix — it returns the exact attrset
that gets serialized to JSON for the binary. Evaluate it to see what your
declarations collapse to:

```bash
nix eval --json .#somethingThatCallsCollect | nix run nixpkgs#jq
```

## Inspect the resolved config

`(dns-manager.lib.generate pkgs).resolve dnsConfig` builds a JSON file of the
fully merged + inherited zones (this runs the binary):

```bash
nix build .#resolved && cat result
```

## Zone files

Check a generated zonefile with `named-checkzone` from the `bind` package:

```bash
nix build .#zoneFiles
named-checkzone example.com result/example.com
```

## octoDNS config

Build the config directory and inspect it, or run `octodns-dump` against a live
provider as described in the octoDNS docs:

```bash
nix build .#cloudflare
cat result/config.yaml
ls result/zones
```
