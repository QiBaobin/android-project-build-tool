{
  description = "An Android build helper";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    zig-overlay.url = "github:mitchellh/zig-overlay";
    zig-overlay.inputs.nixpkgs.follows = "nixpkgs";
    zls-overlay = {
      url = "github:zigtools/zls/0.13.0";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.flake-utils.follows = "flake-utils";
      inputs.zig-overlay.follows = "zig-overlay";
    };
  };
  outputs = { self, nixpkgs, flake-utils, zls-overlay, zig-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        zig = zig-overlay.packages.${system}."0.13.0";
        zls = zls-overlay.packages.${system}.zls;
        abt = pkgs.stdenv.mkDerivation {
          name = "abt";
          src = ./.;
          nativeBuildInputs = [ zig zls ];
          buildPhase = ''
            zig build --global-cache-dir $TMP
          '';
          installPhase = ''
            zig build -p "$out" --release=safe --global-cache-dir $TMP
          '';
        };
      in
      {
        packages.default = abt;
        devShells.env = pkgs.mkShellNoCC {
          packages = [ abt ];
          shellHook = ''
          ${abt}/bin/abt --help
          '';
        };
      }
    );
}
