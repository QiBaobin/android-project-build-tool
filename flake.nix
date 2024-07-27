{
  description = "An Android build helper";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };
  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
      in
      with pkgs; {
        packages.default = stdenv.mkDerivation {
          name = "abt";
          src = ./.;
          buildInputs = [ zig ];
          phases = [];
          configurePhase = ''
            mkdir -p "$TMP/src"
            cp -R "$src"/* "$TMP/src/"
          '';
          buildPhase = ''
            cd "$TMP/src"
            zig build --global-cache-dir .
          '';
          installPhase = ''
            cd "$TMP/src"
            zig build -p "$out" --release=safe --global-cache-dir .
          '';
        };
        apps.default = {
          type = "app";
          program = "${self.packages.${system}.default}/bin/abt";
        };
      }
    );
}
