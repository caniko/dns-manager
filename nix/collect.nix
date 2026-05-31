# Collect declared DNS data into the raw document the `dns-manager` binary
# consumes. This is the Nix↔Rust contract producer: its `builtins.toJSON` is
# exactly the binary's input.
#
# Takes a `dnsConfig` attrset like:
#
#   {
#     inherit (self) nixosConfigurations;   # optional
#     extraConfig = import ./dns.nix;        # optional
#   }
#
# and returns `{ hosts = [ ... ]; extraConfig = { ... }; }`. Only hosts that set
# `networking.domains.enable = true` are included.
{lib}: dnsConfig: let
  nixosConfigurations = dnsConfig.nixosConfigurations or {};
  extraConfig = dnsConfig.extraConfig or null;

  enabledHosts = lib.filter (
    host: host.config.networking.domains.enable or false
  ) (lib.attrValues nixosConfigurations);

  hostRaw = host: let
    d = host.config.networking.domains;
  in {
    defaultTTL = d.defaultTTL;
    baseDomains = d.baseDomains;
    subDomains = d.subDomains;
  };
in
  {
    hosts = map hostRaw enabledHosts;
  }
  // lib.optionalAttrs (extraConfig != null && extraConfig != {}) {
    inherit extraConfig;
  }
