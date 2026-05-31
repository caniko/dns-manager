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

```nix
# extraConfig (e.g. ./dns.nix)
{
  defaultTTL = 60;
  zones."example.com"."" = {
    ns.data = [ "ns1.invalid" "ns2.invalid" ];
    txt = { comment = "policy"; data = "v=spf1 -all"; };
  };
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
}
```

### Cloudflare tokens

`token` is one of:

- `{ type = "env"; name = "CLOUDFLARE_API_TOKEN"; }` — read from the environment.
- `{ type = "file"; path = "/run/secrets/cf"; envName = "CLOUDFLARE_API_TOKEN"; }`
  — read from a file; the output directory gains an `octodns-sync-cloudflare`
  wrapper that exports the token before invoking octoDNS.
- `{ type = "literal"; value = "…"; }` — inline (discouraged; flagged in the config).

### Per-zone Cloudflare settings

Each entry in `zones` accepts `mode` (`"lenient"`/`"strict"`), `manageRecordTypes`
(a `TypeAllowlistFilter`), `excludeRecords` (`[{ name; type; }]`, a
`NameRejectlistFilter`), and extra `processors`.

## 3. octoDNS

The generated directories are ready for `octodns-sync` / `octodns-dump`. For
Cloudflare with a file token, use the generated wrapper:

```bash
result/octodns-sync-cloudflare result --doit
```
