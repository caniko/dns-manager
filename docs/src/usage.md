# Usage

## 1. Declare DNS data

Two sources feed into a `dnsConfig` attrset, both optional:

- **`nixosConfigurations`** — hosts that import `dns-manager.nixosModules.dns` and
  set `networking.domains`. Subdomains inherit records from their most-specific
  base domain.
- **`extraConfig`** — standalone records grouped by zone, for data that does not
  belong to a particular host.

```nix
# A host module
{
  networking.domains = {
    enable = true;
    defaultTTL = 86400;
    baseDomains."example.com" = {
      a.data = "198.51.100.42";
      aaaa.data = "2001:db8::13";
    };
    subDomains = {
      "example.com".mx.data = { preference = 10; exchange = "mail.example.com"; };
      "www.example.com" = { }; # inherits a/aaaa from example.com
    };
  };
}
```

```pkl
// Standalone config (e.g. dns.pkl)
import "../pkl/DnsConfig.pkl" as D

hosts: Listing<D.Host> = new Listing {}
extraConfig: D.ExtraConfig = new D.ExtraConfig {
  defaultTTL = 60
  zones = new Mapping {
    ["example.com"] = new Mapping {
      [""] = new Mapping {
        ["ns"] = new D.Record { data = new Listing { "ns1.invalid"; "ns2.invalid" } }
        ["txt"] = new D.Record { comment = "policy"; data = "v=spf1 -all" }
      }
    }
  }
}
```

Record types: `a`, `aaaa`, `cname`, `alias`, `dname`, `ns`, `txt` (string or list
of strings) and `mx`, `srv`, `soa`, `caa`, `uri`, `tlsa`, `sshfp` (attribute set
or list of attribute sets). Each record also accepts `ttl`, `comment`, `ttlAuto`,
and `proxied`. See [Module options](module-options.md).

## 2. Render

`dns-manager.lib.generate pkgs` returns the render derivations:

```nix
let
  dnsConfig = {
    inherit (self) nixosConfigurations;
    extraConfig = import ./dns.nix;
    redirects = [
      { from = "example.com"; to = "https://www.example.com"; }
    ];
  };
  generate = dns-manager.lib.generate nixpkgs.legacyPackages.${system};
in {
  # BIND zonefiles, one file per zone
  zoneFiles = generate.zonefiles dnsConfig;

  # generic octoDNS config directory (config.yaml + zones/)
  octodns = generate.octodns {
    inherit dnsConfig;
    config.providers.powerdns = {
      class = "octodns_powerdns.PowerDnsProvider";
      host = "ns.dns.invalid";
      api_key = "env/POWERDNS_API_KEY";
    };
    zones."example.com." = { sources = [ "config" ]; targets = [ "powerdns" ]; };
  };

  # Cloudflare octoDNS config directory
  cloudflare = generate.cloudflare {
    inherit dnsConfig;
    token = { type = "env"; name = "CLOUDFLARE_API_TOKEN"; };
    zones."example.com".mode = "lenient";
  };

  # Caddy JSON routes for redirects
  caddyRoutes = generate.caddyRoutes dnsConfig;
}
```

### Sync to multiple providers

Use `generate.sync` or `dns-manager sync --config config.pkl --out result`.
Only zones listed in `zones` are synced. Provider IDs are arbitrary and `targets`
lists the IDs to sync to, in order:

```nix
generate.sync {
  inherit dnsConfig;
  providers = {
    old.kind = "cloudflare";
    new.kind = "desec";
    # new.minTtlOverride = 900; # Only after deSEC approves the lower zone floor.
  };
  zones."example.com" = {
    targets = [ "old" "new" ];
    excludeRecords = [ { name = "@"; type = "A"; } ];
  };
}
```

`kind` can be `cloudflare`, `desec`, `hetzner`, `gcore`, or `bunny`. Each provider
accepts optional plugin `settings`. Set `token = { type = "env"; name = "…"; }`
to override the default environment variable:

| Provider   | Default environment variable | Minimum TTL |
| ---------- | ---------------------------- | ----------: |
| Cloudflare | `CLOUDFLARE_API_TOKEN`       |           1 |
| deSEC      | `DESEC_TOKEN`                |        3600 |
| Hetzner    | `HETZNER_API_TOKEN`          |          60 |
| Gcore      | `GCORE_API_KEY`              |         120 |
| Bunny      | `BUNNY_API_KEY`              |          60 |

Legacy `cloudflare` accepts the same input as before and uses the shared renderer.
For legacy `file` tokens, the output now contains only config and zone files:
the service or launcher must load `path` into `envName` (or the default variable)
before running octoDNS. Do not use literal tokens in Nix or committed configs.

Zone settings also accept `mode` (`"lenient"` or `"strict"`), `manageRecordTypes`,
and extra `processors`. An exclusion matches a relative name, an in-zone full
name, or `@` at the apex, plus the type. An excluded A record does not exclude
MX or TXT at the same name.
Use `extraGlobalConfig` for manager options and custom processor definitions;
generated zone targets, sources, and processors take precedence.

Managed records must meet every target's TTL and capability requirements. Apex
CNAME is always rejected. Proxied records cannot be synced to a non-Cloudflare
target, and apex ALIAS cannot be synced to deSEC or Hetzner. `ttlAuto` keeps a
numeric TTL for other targets and adds automatic-TTL metadata for Cloudflare.

### External Pkl config

For non-Nix callers, author Pkl directly and pass it to the CLI:

```bash
dns-manager resolve --config example/dns.pkl
dns-manager caddy-routes --config example/dns.pkl
dns-manager cloudflare --config example/cloudflare.pkl --out result
```

`pkl/DnsConfig.pkl` is the checked-in contract module for typed standalone
configuration. JSON remains accepted for compatibility, but Pkl is the primary
format.

## 3. octoDNS

Use an octoDNS installation with the target plugins. With Nix, build one with
`pkgs.octodns.withProviders (p: with p; [ cloudflare desec hetzner ])`, using only
the plugins you need. The same three derivations are exposed as
`generate.octodnsProviders`; install the Gcore and Bunny plugins separately.
The Bunny renderer accepts apex ALIAS, but `octodns-bunny` 1.0.0 does not
implement it. Do not use ALIAS with that plugin release.
Load the token environment variables, then preview the changes before applying:

```bash
octodns-sync --config-file result/config.yaml
```

## 4. deSEC CLI and MCP

Read operations use `--token-file`, `DESEC_TOKEN_FILE`, or `DESEC_TOKEN`.
`dns-manager desec domain example.com` includes DNSKEY and DS records in `keys`.
`domains` and `rrsets example.com` list the account's zones and a zone's RRsets.

`domain-create`, `token-mint`, and `token-revoke` require a separate bare token
file specified by `--admin-token-file` or `DESEC_ADMIN_TOKEN_FILE`. Minting
prints only the token ID and path; it never overwrites an existing file.

```bash
dns-manager desec token-mint --name octodns --domain example.com --write \
  --out /run/user/1000/octodns-desec
```

Without `--write`, new tokens cannot modify RRsets. Add `--domain`, `--subname`
and `--type` to narrow writes; an empty subname means the apex. deSEC policies
restrict writes, not reads. `--perm-manage-tokens` lets a token change its own
policies, so do not grant it to an octoDNS sync token.

`dns-manager desec mcp-serve` supports the same credential split. To use its
token-mint tool, supply a private directory (`--token-output-dir` or
`DESEC_TOKEN_OUTPUT_DIR`); the caller can request a new filename inside that
directory. Neither the CLI nor MCP exposes RRset writes.
