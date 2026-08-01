{
  description = "Declarative DNS data → BIND zonefiles and octoDNS, resolved and rendered in Rust";

  inputs = {
    rs-harbor.url = "github:caniko/rs-harbor/e2778ff3beca1bd4c1f5183313251d1fb5b46dd6";

    nixpkgs.follows = "rs-harbor/nixpkgs";
    rust-overlay.follows = "rs-harbor/rust-overlay";
    crane.follows = "rs-harbor/crane";

    treefmt-nix.url = "github:numtide/treefmt-nix";
    treefmt-nix.inputs.nixpkgs.follows = "nixpkgs";

    git-hooks.url = "github:cachix/git-hooks.nix";
    git-hooks.inputs.nixpkgs.follows = "nixpkgs";

    plinth = {
      url = "git+https://codeberg.org/caniko/plinth.git?ref=refs/heads/trunk";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    nix-manager-core = {
      url = "git+https://codeberg.org/caniko/nix-manager-core";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.rs-harbor.follows = "rs-harbor";
      inputs.rust-overlay.follows = "rust-overlay";
      inputs.crane.follows = "crane";
      inputs.treefmt-nix.follows = "treefmt-nix";
      inputs.git-hooks.follows = "git-hooks";
    };
  };

  outputs = inputs: import ./nix inputs;
}
