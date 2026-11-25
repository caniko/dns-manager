# Render derivations: thin wrappers that serialize collected DNS config to Pkl
# and run the `dns-manager` binary at build time.
#
# Exposed to consumers as `(dns-manager.lib.generate pkgs).{ zonefiles, octodns,
# sync, cloudflare, octodnsProviders, caddyRoutes, resolve }`.
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
  renderSync = subcommand: name: settings:
    renderDir subcommand name (settings // {dnsConfig = collect settings.dnsConfig;});
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

  # Multi-provider YAML source and target config. Tokens are loaded at runtime.
  sync = renderSync "sync" "dns-sync";

  # Compatibility input; the binary adapts it to the same sync renderer.
  cloudflare = renderSync "cloudflare" "dns-cloudflare";

  octodnsProviders = {
    inherit (pkgs.octodns-providers) cloudflare desec hetzner;
  };
}
