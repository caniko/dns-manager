# Flake outputs assembly — delegates to nix-manager-core's reusable scaffold
# for generic outputs (package, checks, devShells, treefmt) and provides
# domain-specific outputs (module, collect/generate, docs, website, apps) via
# extraOutputs.
{
  self,
  nixpkgs,
  rs-harbor,
  rust-overlay,
  treefmt-nix,
  git-hooks,
  nix-manager-core,
  plinth,
  ...
}:
nix-manager-core.lib.mkManagerOutputs {
  inherit self nixpkgs rs-harbor rust-overlay treefmt-nix git-hooks;
  crateName = "dns-manager";
  srcDir = ../.;

  extraDevShellPackages = pkgs: with pkgs; [bind octodns mdbook];

  extraOutputs = {
    self,
    lib,
    forAllSystems,
    pkgsFor,
    cargoFor,
  }: let
    module = import ./module.nix;
    collect = import ./collect.nix {inherit lib;};
  in {
    nixosModules = {
      dns = module;
      default = module;
    };

    lib = {
      inherit collect;
      generate = pkgs:
        import ./generate.nix {
          inherit lib pkgs;
          dns-manager = (cargoFor pkgs.stdenv.hostPlatform.system).package;
        };
    };

    packages = forAllSystems (system: let
      pkgs = pkgsFor system;
      website = plinth.lib.${system}.mkProjectSite {
        pname = "dns-manager-website";
        domain = "dns-manager.tartanoglu.com";
        configPath = ../website/plinth-project.toml;
        docsPackage = self.packages.${system}.docs;
      };
    in {
      docs = import ./docs.nix {inherit pkgs lib module;};
      inherit website;
      site = website;
    });

    apps = forAllSystems (system: {
      deploy-pages = plinth.lib.${system}.mkDeployPagesApp {
        domain = "dns-manager.tartanoglu.com";
      };
    });

    devShells = forAllSystems (system: let
      pkgs = pkgsFor system;
      toolchain = rs-harbor.lib.mkToolchain {inherit pkgs;};
      cross = rs-harbor.lib.mkCross {inherit pkgs system;};
    in {
      docs = rs-harbor.lib.mkDocsShell {
        inherit pkgs cross;
        inherit (toolchain) craneLib;
        packages = with pkgs; [
          bind
          mdbook
          octodns
          (plinth.packages.${system}.plinth-project)
        ];
        extraShellHook = ''
          echo "Project site: plinth-project serve --config website/plinth-project.toml"
          echo "Documentation: mdbook serve docs"
        '';
      };
    });

    templates.default = {
      path = ../example;
      description = "A dns-manager example flake";
      welcomeText = ''
        A dns-manager example: declare DNS data and render zonefiles / octoDNS / Cloudflare config.
      '';
    };
  };
}
