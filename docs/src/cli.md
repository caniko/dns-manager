# CLI

`dns-manager` is the Rust binary the Nix layer calls at build time. You can also
run it directly on a JSON document — the same format the Nix `collect` layer
produces.

```
dns-manager <resolve|zonefile|octodns|cloudflare> --config <file|-> [--out <dir>]
```

- `--config` reads the input JSON from a file, or from stdin when `-` (default).
- `--out` is the output directory (for everything except `resolve`, which prints
  JSON to stdout).

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
  }
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
