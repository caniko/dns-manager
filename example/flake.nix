{
  description = "An example of how to use dns-manager";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    dns-manager.url = "path:..";
    dns-manager.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = {
    self,
    nixpkgs,
    dns-manager,
  }: let
    forAllSystems = nixpkgs.lib.genAttrs [
      "x86_64-linux"
      "aarch64-linux"
    ];
    dnsConfig = {
      inherit (self) nixosConfigurations;
      extraConfig = import ./dns.nix;
    };
  in {
    nixosConfigurations = {
      host1 = nixpkgs.lib.nixosSystem {
        system = "x86_64-linux";
        modules = [
          dns-manager.nixosModules.dns
          ./hosts/host1.nix
        ];
      };
      host2 = nixpkgs.lib.nixosSystem {
        system = "x86_64-linux";
        modules = [
          dns-manager.nixosModules.dns
          ./hosts/host2.nix
        ];
      };
    };

    packages = forAllSystems (
      system: let
        generate = dns-manager.lib.generate nixpkgs.legacyPackages.${system};
      in {
        # nix build .#resolved && cat result
        resolved = generate.resolve dnsConfig;

        # nix build .#zoneFiles
        zoneFiles = generate.zonefiles dnsConfig;

        # nix build .#octodns
        octodns = generate.octodns {
          inherit dnsConfig;
          config = {
            providers = {
              powerdns = {
                class = "octodns_powerdns.PowerDnsProvider";
                host = "ns.dns.invalid";
                api_key = "env/POWERDNS_API_KEY";
              };
            };
          };
          zones = {
            "example.com." = {
              sources = ["config"];
              targets = ["powerdns"];
            };
            "example.org." = {
              sources = ["config"];
              targets = ["powerdns"];
            };
          };
        };

        # nix build .#cloudflare
        cloudflare = generate.cloudflare {
          inherit dnsConfig;
          token = {
            type = "env";
            name = "CLOUDFLARE_API_TOKEN";
          };
          zones."example.com".mode = "lenient";
        };
      }
    );
  };
}
