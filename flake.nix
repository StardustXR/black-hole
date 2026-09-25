{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    crane.url = "github:ipetkov/crane";
    rust-overlay = {
        url = "github:oxalica/rust-overlay";
        inputs.nixpkgs.follows = "nixpkgs";
    };
  };


  outputs = { self, nixpkgs, crane, rust-overlay }:
  let supportedSystems = [ "aarch64-linux" "x86_64-linux" ];
      forAllSystems = nixpkgs.lib.genAttrs supportedSystems;
      nixpkgsFor = forAllSystems (system: import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
      });
  in {
    packages = forAllSystems (system: let pkgs = nixpkgsFor.${system}; in {
      default =
      let
      craneLib = (crane.mkLib pkgs).overrideToolchain (p: p.rust-bin.stable.latest.default.override {
        targets = [ "x86_64-unknown-linux-musl" ];
      });
      in craneLib.buildPackage {
        src = ./.;
        cargoLock = ./Cargo.lock;

        CARGO_BUILD_TARGET = "x86_64-unknown-linux-musl";
        CARGO_BUILD_RUSTFLAGS = "-C target-feature=+crt-static";

        STARDUST_RES_PREFIXES = pkgs.stdenvNoCC.mkDerivation {
          name = "data";
          src = ./.;

          buildPhase = "cp -r $src/data $out";
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
