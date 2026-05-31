# Flake outputs assembly. `flake.nix` is a thin shim that calls `import ./nix`.
{
  self,
  nixpkgs,
  rs-harbor,
  rust-overlay,
  treefmt-nix,
  git-hooks,
  ...
}: let
  inherit (nixpkgs) lib;

  systems = [
    "x86_64-linux"
    "aarch64-linux"
    "x86_64-darwin"
    "aarch64-darwin"
  ];
  forAllSystems = f: lib.genAttrs systems f;

  pkgsFor = system:
    import nixpkgs {
      inherit system;
      overlays = [(import rust-overlay)];
    };
  cargoFor = system:
    import ./package.nix {
      pkgs = pkgsFor system;
      inherit rs-harbor;
    };

  module = import ./module.nix;
  collect = import ./collect.nix {inherit lib;};
in {
  nixosModules = {
    dns = module;
    default = module;
  };

  # Nix-side helpers consumers use.
  lib = {
    inherit collect;
    # `(dns-manager.lib.generate pkgs).{ zonefiles, octodns, cloudflare, resolve }`
    generate = pkgs:
      import ./generate.nix {
        inherit lib pkgs;
        dns-manager = self.packages.${pkgs.system}.dns-manager;
      };
  };

  packages = forAllSystems (
    system: let
      pkgs = pkgsFor system;
      cargo = cargoFor system;
    in {
      default = cargo.package;
      dns-manager = cargo.package;
      docs = import ./docs.nix {inherit pkgs lib module;};
      site = self.packages.${system}.docs;
    }
  );

  checks = forAllSystems (
    system: let
      pkgs = pkgsFor system;
      cargo = cargoFor system;
      treefmtEval = treefmt-nix.lib.evalModule pkgs ./treefmt.nix;
    in
      (import ./checks.nix {
        inherit
          (cargo)
          craneLib
          commonArgs
          cargoArtifacts
          src
          ;
      })
      // {
        formatting = treefmtEval.config.build.check self;
      }
  );

  devShells = forAllSystems (
    system: let
      pkgs = pkgsFor system;
      cargo = cargoFor system;
      treefmtEval = treefmt-nix.lib.evalModule pkgs ./treefmt.nix;
      pre-commit-check = git-hooks.lib.${system}.run {
        src = ../.;
        install.enable = false;
        hooks = import ./pre-commit.nix {
          inherit pkgs;
          treefmtWrapper = treefmtEval.config.build.wrapper;
        };
      };
    in {
      default = cargo.craneLib.devShell {
        checks = self.checks.${system};
        packages = with pkgs;
          [
            cargo-nextest
            pre-commit
            rust-analyzer
            bind
            octodns
            mdbook
          ]
          ++ pre-commit-check.enabledPackages;
        shellHook = pre-commit-check.shellHook;
      };
    }
  );

  formatter = forAllSystems (
    system: (treefmt-nix.lib.evalModule (pkgsFor system) ./treefmt.nix).config.build.wrapper
  );

  templates.default = {
    path = ../example;
    description = "A dns-manager example flake";
    welcomeText = ''
      A dns-manager example: declare DNS data and render zonefiles / octoDNS / Cloudflare config.
    '';
  };
}
