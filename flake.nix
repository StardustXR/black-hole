{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    crane = {
      inputs.nixpkgs.follows = "nixpkgs";
      url = "github:ipetkov/crane";
    };
  };


  outputs = { self, nixpkgs, crane }:
  let supportedSystems = [ "aarch64-linux" "x86_64-linux" ];
      forAllSystems = nixpkgs.lib.genAttrs supportedSystems;
      nixpkgsFor = forAllSystems (system: import nixpkgs { inherit system; });
  in {
    packages = forAllSystems (system: let pkgs = nixpkgsFor.${system}; in {
      default =
      let
      craneLib = (crane.mkLib pkgs).overrideToolchain (p: p.rust-bin.stable.latest.default.override {
        targets = [ "x86_64-unknown-linux-musl" ];
      });
      in craneLib.buildPackage {
        src = ./.;

        CARGO_BUILD_TARGET = "x86_64-unknown-linux-musl";
        CARGO_BUILD_RUSTFLAGS = "-C target-feature=+crt-static";

        STARDUST_RES_PREFIXES = pkgs.stdenvNoCC.mkDerivation {
          name = "resources";
          src = ./.;

          buildPhase = "cp -r $src/res $out";
        };
      };
    });

    devShells = forAllSystems (system: let pkgs = nixpkgsFor.${system}; in {
      default = pkgs.mkShell {
        nativeBuildInputs = with pkgs; [
          cargo
          rustc
        ];
      };
    });
  };
}
