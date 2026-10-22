{
  description = "Dev shell flake starter";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
      in {
        packages.default = pkgs.writeShellApplication {
          name = "hello";
          text = ''
            echo "Hello from Nix!"
          '';
        };

        apps.default = {
          type = "app";
          program = "${self.packages.${system}.default}/bin/hello";
        };

        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            git
            curl
            jq
            nodejs_22
            python3
          ];
          shellHook = ''
            echo "Nix dev shell ready (nodejs + python)."
          '';
        };
      });
}
