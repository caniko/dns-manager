# Render derivations: thin wrappers that serialize collected DNS config to JSON
# and run the `dns-manager` binary at build time.
#
# Exposed to consumers as `(dns-manager.lib.generate pkgs).{ zonefiles, octodns,
# cloudflare, resolve }`.
{
  lib,
  pkgs,
  dns-manager,
}: let
  collect = import ./collect.nix {inherit lib;};
  bin = "${dns-manager}/bin/dns-manager";
  inputFile = name: value: pkgs.writeText name (builtins.toJSON value);

  renderDir = subcommand: name: input:
    pkgs.runCommand name {} ''
      mkdir -p "$out"
      ${bin} ${subcommand} --config ${inputFile "${name}-input.json" input} --out "$out"
    '';
in {
  # Pure-Nix raw view of the collected config (no binary needed).
  inherit collect;

  # Resolved config as a JSON file (runs the binary).
  resolve = dnsConfig:
    pkgs.runCommand "dns-resolved.json" {} ''
      ${bin} resolve --config ${inputFile "dns-input.json" (collect dnsConfig)} > "$out"
    '';

  # BIND zonefiles, one file per zone.
  zonefiles = dnsConfig: renderDir "zonefile" "dns-zones" (collect dnsConfig);

  # Generic octoDNS config directory. `settings` mirrors the legacy octodnsConfig
  # input: { dnsConfig, config ? {}, zones ? {}, manager ? null }.
  octodns = settings: renderDir "octodns" "dns-octodns" (settings // {dnsConfig = collect settings.dnsConfig;});

  # Cloudflare octoDNS config directory. `settings`:
  #   { dnsConfig, token, zones ? null, extraProviderSettings ? {}, extraGlobalConfig ? {} }
  # For file tokens, an `octodns-sync-cloudflare` wrapper is added that exports
  # the token from its file before invoking octoDNS.
  cloudflare = settings: let
    input =
      settings
      // {
        dnsConfig = collect settings.dnsConfig;
      };
    token = settings.token;
    isFile = (token.type or "") == "file";
    wrapper = pkgs.writeShellScript "octodns-sync-cloudflare" ''
      set -eu
      export ${token.envName or "CLOUDFLARE_API_TOKEN"}="$(cat ${lib.escapeShellArg (token.path or "")})"
      exec ${pkgs.octodns}/bin/octodns-sync --config-file "$1/config.yaml" "''${@:2}"
    '';
  in
    pkgs.runCommand "dns-cloudflare" {} ''
      mkdir -p "$out"
      ${bin} cloudflare --config ${inputFile "cloudflare-input.json" input} --out "$out"
      ${lib.optionalString isFile ''ln -s ${wrapper} "$out/octodns-sync-cloudflare"''}
    '';
}
