# Migration: pre-Rust → Rust dns-manager

This release moves all DNS logic from Nix into a Rust binary and **redesigns the
public flake API**. The data you declare is unchanged; the entry points to render
it are renamed.

## Flake API mapping

| Before                                            | After                                                         |
| ------------------------------------------------- | ------------------------------------------------------------- |
| `dns-manager.utils.generate pkgs`                 | `dns-manager.lib.generate pkgs`                               |
| `(utils.generate pkgs).zoneFiles cfg`             | `(lib.generate pkgs).zonefiles cfg`                           |
| `(utils.generate pkgs).octodnsConfig settings`    | `(lib.generate pkgs).octodns settings`                        |
| `(utils.generate pkgs).cloudflareConfig settings` | `(lib.generate pkgs).cloudflare settings`                     |
| `dns-manager.utils.debug.config cfg`              | `(lib.generate pkgs).resolve cfg` (a derivation → JSON)       |
| `dns-manager.utils.debug.host h`                  | `dns-manager.lib.collect { nixosConfigurations.h = h; }`      |
| `dns-manager.utils.octodns.generateZoneAttrs ts`  | inline `{ sources = [ "config" ]; targets = ts; }`            |
| `dns-manager.nixosModules.dns`                    | unchanged                                                     |
| `packages.<sys>.tests`                            | gone — tests are cargo `checks`                               |
| — (new)                                           | `packages.<sys>.dns-manager` (the CLI), `packages.<sys>.docs` |

The `settings` attrsets for `octodns` and `cloudflare` are unchanged
(`{ dnsConfig, token, zones, … }`), as are the `networking.domains` declarations
and the `extraConfig` shape.

## Output compatibility

The **Cloudflare output directory layout is unchanged**: `config.yaml`,
`zones/<zone>.yaml`, and (for file tokens) the executable `octodns-sync-cloudflare`
wrapper. Anything that consumes that directory at runtime — e.g. a
`cloudflare-octodns` service — keeps working; only the Nix call site changes.

YAML/zonefile _formatting_ may differ byte-for-byte from the old Nix renderer, but
the content is equivalent and validated (`named-checkzone`, octoDNS parse).

## Behavioural notes

- Validation (matching base domain, comment ≤ 100 bytes, `ttlAuto` vs explicit
  `ttl`, `proxied` only on A/AAAA/CNAME/ALIAS) now happens in the binary at build
  time and reports **all** violations at once, instead of failing module
  evaluation one assertion at a time.
- Hosts with `networking.domains.enable = false` are excluded entirely from the
  collected config (previously their base-domain names could still leak into apex
  computation).
- Trailing-dot handling for `cname`/`srv`/`ns`/`mx`/`alias`/`dname` targets is now
  idempotent. The legacy Nix appended a dot unconditionally, so an already-dotted
  target produced an invalid double dot (`foo.` → `foo..`); the binary
  canonicalizes to a single dot. Output is identical for the usual un-dotted
  input.
- TXT records are split into ≤255-**byte** character-strings (RFC 4408 octets),
  matching the byte semantics of the legacy Nix and keeping non-ASCII TXT within
  the DNS limit.
- A base domain's `soa` is no longer inherited onto subdomains; an SOA only comes
  from a zone apex / `extraConfig`, as before.

## Downstream: canix-toolbelt

`canix-toolbelt` consumes `utils.generate.cloudflareConfig`. Update its call to
`(dns-manager.lib.generate pkgs).cloudflare` (same `settings`). The generated
directory contract is identical, so the `cloudflare-octodns` service and
`octodnsConfigLocal` wrapper require no further change.
