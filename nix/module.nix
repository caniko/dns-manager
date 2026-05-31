# The `networking.domains` NixOS module.
#
# This module is intentionally thin: it only *declares* DNS data. All logic —
# domain matching, sub→base inheritance, normalization, validation, and
# rendering — lives in the `dns-manager` Rust binary, which consumes the data
# collected from this module (see `nix/collect.nix`).
{lib, ...}: let
  recordModule = lib.types.submodule {
    options = {
      data = lib.mkOption {
        type = lib.types.anything;
        description = ''
          The record payload. Its shape depends on the record type: a string or
          list of strings for address/name/text records (`a`, `aaaa`, `cname`,
          `alias`, `dname`, `ns`, `txt`), or an attribute set — or list thereof —
          for structured records (`mx`, `srv`, `soa`, `caa`, `uri`, `tlsa`,
          `sshfp`). Coercion and validation are performed by the `dns-manager`
          binary, not here.
        '';
      };
      ttl = lib.mkOption {
        type = lib.types.nullOr lib.types.int;
        default = null;
        example = 86400;
        description = "Explicit TTL in seconds; when null the zone/host `defaultTTL` is used.";
      };
      comment = lib.mkOption {
        type = lib.types.nullOr lib.types.str;
        default = null;
        description = "Optional provider-side / zonefile comment (at most 100 bytes).";
      };
      ttlAuto = lib.mkOption {
        type = lib.types.bool;
        default = false;
        description = "Let the provider manage the TTL automatically; mutually exclusive with an explicit `ttl`.";
      };
      proxied = lib.mkOption {
        type = lib.types.bool;
        default = false;
        description = "Whether supported providers proxy this record. Valid only on A, AAAA, CNAME, and ALIAS.";
      };
    };
  };

  # domain/fqdn -> record type -> record
  domainsType = lib.types.attrsOf (lib.types.attrsOf recordModule);
in {
  options.networking.domains = {
    enable = lib.mkEnableOption "declarative DNS data managed by dns-manager";

    defaultTTL = lib.mkOption {
      type = lib.types.int;
      default = 3600;
      description = "TTL applied to this host's records that do not set one explicitly.";
    };

    baseDomains = lib.mkOption {
      type = domainsType;
      default = {};
      description = ''
        Template records that subdomains inherit from. A base domain's records
        are never emitted on their own — a subdomain inherits each non-empty
        base record type's `data` and `ttl` unless it overrides them.
      '';
    };

    subDomains = lib.mkOption {
      type = domainsType;
      default = {};
      description = ''
        Concrete records keyed by fully-qualified name. Each subdomain inherits
        the records of its most-specific matching base domain.
      '';
    };
  };
}
