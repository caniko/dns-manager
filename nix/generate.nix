# Render derivations: thin wrappers that serialize collected DNS config to Pkl
# and run the `dns-manager` binary at build time.
#
# Exposed to consumers as `(dns-manager.lib.generate pkgs).{ zonefiles, octodns,
# cloudflare, caddyRoutes, resolve }`.
{
  lib,
  pkgs,
  dns-manager,
}: let
  collect = import ./collect.nix {inherit lib;};
  pkl = import ./to-pkl.nix {inherit lib;};
  bin = "${dns-manager}/bin/dns-manager";
  inputFile = name: value: pkgs.writeText name (pkl.moduleToPkl value);
  pklEvalEnv = {
    SSL_CERT_FILE = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";
  };

  renderDir = subcommand: name: input:
    pkgs.runCommand name pklEvalEnv ''
      mkdir -p "$out"
      ${bin} ${subcommand} --config ${inputFile "${name}-input.pkl" input} --out "$out"
    '';
in {
  # Pure-Nix raw view of the collected config (no binary needed).
  inherit collect;

  # Resolved config as a JSON file (runs the binary).
  resolve = dnsConfig:
    pkgs.runCommand "dns-resolved.json" pklEvalEnv ''
      ${bin} resolve --config ${inputFile "dns-input.pkl" (collect dnsConfig)} > "$out"
    '';

  # BIND zonefiles, one file per zone.
  zonefiles = dnsConfig: renderDir "zonefile" "dns-zones" (collect dnsConfig);

  # Caddy JSON routes for HTTP redirect intents.
  caddyRoutes = dnsConfig:
    builtins.fromJSON (builtins.readFile (pkgs.runCommand "dns-caddy-routes.json" pklEvalEnv ''
      ${bin} caddy-routes --config ${inputFile "dns-caddy-routes-input.pkl" (collect dnsConfig)} > "$out"
    ''));

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
    pkgs.runCommand "dns-cloudflare" pklEvalEnv ''
      mkdir -p "$out"
      ${bin} cloudflare --config ${inputFile "cloudflare-input.pkl" input} --out "$out"
      ${lib.optionalString isFile ''ln -s ${wrapper} "$out/octodns-sync-cloudflare"''}
    '';
}
