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
        wasmTarget = "wasm32-unknown-unknown";

        # Toolchain for proxy (musl static binary)
        proxyToolchain = pkgs.rust-bin.stable.latest.minimal.override {
          targets = [ muslTarget ];
        };

        # Toolchain for plugin (wasm)
        wasmToolchain = pkgs.rust-bin.stable.latest.minimal.override {
          targets = [ wasmTarget ];
        };

        proxyCraneLib = (crane.mkLib pkgs).overrideToolchain (_: proxyToolchain);
        wasmCraneLib = (crane.mkLib pkgs).overrideToolchain (_: wasmToolchain);

        # Use crane's cleanCargoSource for proper filtering
        src = proxyCraneLib.cleanCargoSource ./.;

        nativeBuildInputs = [ pkgs.pkg-config ];
      in
      rec {
        packages = {
          # DAWN proxy binary (static musl)
          proxy = proxyCraneLib.buildPackage {
            inherit nativeBuildInputs src;
            pname = "dawn";
            version = "0.1.0";
            cargoExtraArgs = "-p dawn";

            CARGO_BUILD_TARGET = muslTarget;
            CARGO_BUILD_RUSTFLAGS = "-C target-feature=+crt-static";
            HOST_CC = "${pkgs.stdenv.cc}/bin/cc";
          };

          # DAWN tester binary (static musl)
          tester = proxyCraneLib.buildPackage {
            inherit nativeBuildInputs src;
            pname = "dawn-tester";
            version = "0.1.0";
            cargoExtraArgs = "-p dawn_tester";

            CARGO_BUILD_TARGET = muslTarget;
            CARGO_BUILD_RUSTFLAGS = "-C target-feature=+crt-static";
            HOST_CC = "${pkgs.stdenv.cc}/bin/cc";
          };

          # Doubler WASM plugin
          doublerPlugin = wasmCraneLib.buildPackage {
            inherit src;
            pname = "dawn_doubler";
            version = "0.1.0";
            cargoExtraArgs = "-p dawn_doubler";

            CARGO_BUILD_TARGET = wasmTarget;

            # WASM doesn't need linking
            doCheck = false;

            installPhaseCommand = ''
              mkdir -p $out/lib
              cp target/${wasmTarget}/release/dawn_doubler.wasm $out/lib/
            '';
          };

          # Bundle: proxy, tester, and all plugins
          bundle = pkgs.symlinkJoin {
            name = "dawn-bundle";
            paths = [ packages.proxy packages.tester packages.doublerPlugin ];
          };

          # Full bundle as a zip file with binaries, plugins, and data
          fullBundle = pkgs.runCommand "dawn-full-bundle" {
            nativeBuildInputs = [ pkgs.zip ];
          } ''
            mkdir -p $out dawn-bundle/bin dawn-bundle/lib dawn-bundle/data

            # Copy binaries
            cp ${packages.proxy}/bin/dawn dawn-bundle/bin/
            cp ${packages.tester}/bin/dawn-tester dawn-bundle/bin/

            # Copy WASM plugins
            cp ${packages.doublerPlugin}/lib/*.wasm dawn-bundle/lib/

            # Copy data files
            cp -r ${./data}/* dawn-bundle/data/

            # Create zip
            cd dawn-bundle
            zip -r $out/dawn-bundle.zip .
          '';

          default = packages.bundle;
        };

        devShells.default = pkgs.mkShell {
          nativeBuildInputs = nativeBuildInputs ++ [
            (pkgs.rust-bin.stable.latest.default.override {
              extensions = [ "rust-src" "rustfmt" "rust-analyzer" "clippy" ];
              targets = [ muslTarget wasmTarget ];
            })
            pkgs.wasmtime
          ];
        };
      }
    );
}
