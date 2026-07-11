# CLI

`dns-manager` is the Rust binary the Nix layer calls at build time. Nix
generation serializes inputs as Pkl and the CLI evaluates them with pklx.
Standalone users should author Pkl against `pkl/DnsConfig.pkl`. You can still
run it directly on a JSON document by passing `--config -` or a `.json` file.

```
dns-manager <resolve|zonefile|octodns|cloudflare|caddy-routes> --config <file|-> [--out <dir>]
```

- `--config` reads Pkl from files by default; `.json` files and stdin are read
  as JSON for compatibility.
- `--out` is the output directory (for everything except `resolve`, which prints
  JSON to stdout, and `caddy-routes`, which prints Caddy JSON routes).

## Input format

The top-level document is:

```json
{
  "hosts": [
    {
      "defaultTTL": 3600,
      "baseDomains": { "example.com": { "a": { "data": "198.51.100.42" } } },
      "subDomains": { "www.example.com": {} }
    }
  ],
  "extraConfig": {
    "defaultTTL": 60,
    "zones": { "example.com": { "": { "ns": { "data": ["ns1.invalid"] } } } }
  },
  "redirects": [
    { "from": "example.com", "to": "https://www.example.com", "status": 301, "preservePath": true }
  ]
}
```

A record is `{ "data": <payload>, "ttl"?: int, "comment"?: string,
"ttlAuto"?: bool, "proxied"?: bool }`.

The `octodns` and `cloudflare` subcommands take the document above under a
`dnsConfig` key, alongside their own settings:

```json
{ "dnsConfig": { ... }, "token": { "type": "env", "name": "CLOUDFLARE_API_TOKEN" } }
```

## Subcommands

- **`resolve`** — print the merged, inherited, validated zones as JSON (debugging).
- **`zonefile`** — write one BIND zonefile per zone into `--out`.
- **`octodns`** — write `config.yaml` + `zones/<zone>` (BIND, with a dummy SOA) into `--out`.
- **`cloudflare`** — write `config.yaml` + `zones/<zone>.yaml` (YAML provider) into `--out`.
- **`caddy-routes`** — print Caddy JSON routes for `redirects`.

Examples:

```bash
dns-manager resolve --config example/dns.pkl
dns-manager cloudflare --config example/cloudflare.pkl --out result
```
