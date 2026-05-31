# mdBook documentation, with the `networking.domains` module options generated
# from the module itself via nixosOptionsDoc.
{
  pkgs,
  lib,
  module,
}: let
  eval = lib.evalModules {modules = [module];};
  optionsDoc = pkgs.nixosOptionsDoc {inherit (eval) options;};
in
  pkgs.runCommand "dns-manager-docs" {nativeBuildInputs = [pkgs.mdbook];} ''
    mkdir -p src
    cp -r ${../docs/src}/. src/
    cp ${optionsDoc.optionsCommonMark} src/module-options.md
    cp ${../docs/book.toml} book.toml

    mdbook build -d "$out"
  ''
