{lib}: let
  pkl = import ./to-pkl.nix {inherit lib;};
in
  pkl.moduleToPkl {
    extraConfig = {
      defaultTTL = 300;
      zones = {
        "tartanoglu.com" = {
          "" = {
            txt.data = "apex";
          };
          "_dmarc" = {
            txt.data = "v=DMARC1";
          };
          "mail._domainkey" = {
            txt.data = "v=DKIM1";
          };
        };
      };
    };
  }
