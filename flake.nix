{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
    crane.url = "github:ipetkov/crane";
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay, crane }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = (import nixpkgs) {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

        muslTarget = "x86_64-unknown-linux-musl";

        rustToolchain = pkgs.rust-bin.stable.latest.minimal.override {
          targets = [ muslTarget ];
        };

        craneLib = (crane.mkLib pkgs).overrideToolchain (_: rustToolchain);

        nativeBuildInputs = [ pkgs.pkg-config ];
        buildInputs = [ ];
      in
      rec {
        defaultPackage = craneLib.buildPackage {
          inherit nativeBuildInputs buildInputs;
          src = ./.;

          CARGO_BUILD_TARGET = muslTarget;
          CARGO_BUILD_RUSTFLAGS = "-C target-feature=+crt-static";
          HOST_CC = "${pkgs.stdenv.cc}/bin/cc";
        };

        devShell = pkgs.mkShell {
          inherit buildInputs;
          nativeBuildInputs = nativeBuildInputs ++ [
            (pkgs.rust-bin.stable.latest.default.override {
              extensions = [ "rust-src" "rustfmt" "rust-analyzer" "clippy" ];
            })
          ];
        };
      }
    );
}
