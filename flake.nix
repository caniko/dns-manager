{
  description = "Declarative DNS data → BIND zonefiles and octoDNS, resolved and rendered in Rust";

  inputs = {
    rs-harbor.url = "git+https://codefloe.com/caniko/rs-harbor.git?ref=trunk&rev=7fa1c2104dab4e1dbaa1aaa6df84bba815aa282d";

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
