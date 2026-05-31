# crane build of the dns-manager workspace, using the rs-harbor toolchain.
{
  pkgs,
  rs-harbor,
}: let
  toolchain = rs-harbor.lib.mkToolchain {inherit pkgs;};
  inherit (toolchain) craneLib;

  src = craneLib.cleanCargoSource ../.;

  commonArgs = {
    inherit src;
    strictDeps = true;
    pname = "dns-manager";
    version = "0.1.0";
  };

  cargoArtifacts = craneLib.buildDepsOnly commonArgs;

  package = craneLib.buildPackage (
    commonArgs
    // {
      inherit cargoArtifacts;
      doCheck = false; # tests run as a dedicated `nextest` check
      meta.mainProgram = "dns-manager";
    }
  );
in {
  inherit
    package
    craneLib
    commonArgs
    cargoArtifacts
    src
    toolchain
    ;
}
