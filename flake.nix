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
        # Toolchain for proxy (musl static binary)
        proxyToolchain = pkgs.rust-bin.stable.latest.minimal.override {
          targets = [ muslTarget ];
        };
        proxyCraneLib = (crane.mkLib pkgs).overrideToolchain (_: proxyToolchain);

        # Toolchain for plugin (wasm)
        wasmTarget = "wasm32-unknown-unknown";
        wasmToolchain = pkgs.rust-bin.stable.latest.minimal.override {
          targets = [ wasmTarget ];
        };
        wasmCraneLib = (crane.mkLib pkgs).overrideToolchain (_: wasmToolchain);

        # Use crane's cleanCargoSource for proper filtering
        src = proxyCraneLib.cleanCargoSource ./.;

        # Common functionality for building static rust binaries (musl)
        buildPackage = { path, nativeBuildInputs }:
          let
            crate = wasmCraneLib.crateNameFromCargoToml { cargoToml = "${path}/Cargo.toml"; };
          in
          proxyCraneLib.buildPackage {
            inherit src;
            inherit (crate) pname;
            cargoExtraArgs = "-p ${crate.pname}";
            CARGO_BUILD_TARGET = muslTarget;
            CARGO_BUILD_RUSTFLAGS = "-C target-feature=+crt-static";
            HOST_CC = "${pkgs.stdenv.cc}/bin/cc";
          };
        # Function to build plugin derivations
        buildPlugin = path:
          let
            crate = wasmCraneLib.crateNameFromCargoToml { cargoToml = "${path}/Cargo.toml"; };
          in
          wasmCraneLib.buildPackage {
            inherit src;
            inherit (crate) pname version;
            cargoExtraArgs = "-p ${crate.pname}";
            CARGO_BUILD_TARGET = wasmTarget;
            # WASM doesn't need linking
            doCheck = false;
            installPhaseCommand = ''
              mkdir -p $out/lib
              cp target/${wasmTarget}/release/${crate.pname}.wasm $out/lib/
            '';
          };
      in
      rec {
        packages = {
          # DAWN proxy and tester binaries
          dawn = buildPackage {
            path = ./dawn;
            nativeBuildInputs = with pkgs; [ pkg-config ];
          };

          # WASM plugins
          ## DNS Doubler
          doublerPlugin = buildPlugin ./plugins/doubler;
          ## Inverse query
          iqueryPlugin = buildPlugin ./plugins/iquery;

          # Test runner script that tests all plugins
          test-all = pkgs.writeShellScriptBin "dawn-test-all" ''
            exec ${packages.dawn}/bin/dawn-tester \
              --plugins "${packages.doublerPlugin}/lib/dawn_doubler.wasm,${packages.iqueryPlugin}/lib/dawn_iquery.wasm" \
              --domains "${./data}/censored.txt" \
              --forged-ipv4 "${./data}/forged.ipv4" \
              --forged-ipv6 "${./data}/forged.ipv6" \
              "$@"
          '';

          # Full bundle as a zip file with binaries, plugins, and data
          release = pkgs.runCommand "dawn-full-bundle"
            {
              nativeBuildInputs = [ pkgs.zip ];
            } ''
            mkdir -p $out dawn-bundle/bin dawn-bundle/lib dawn-bundle/data

            # Copy binaries
            cp ${packages.dawn}/bin/dawn dawn-bundle/bin/
            cp ${packages.dawn}/bin/dawn-tester dawn-bundle/bin/

            # Copy WASM plugins
            cp ${packages.doublerPlugin}/lib/*.wasm dawn-bundle/lib/
            cp ${packages.iqueryPlugin}/lib/*.wasm dawn-bundle/lib/

            # Copy data files
            cp -r ${./data}/* dawn-bundle/data/

            # Create zip
            cd dawn-bundle
            zip -r $out/dawn-bundle.zip .
          '';

          default = packages.release;
        };

        devShells.default = pkgs.mkShell {
          nativeBuildInputs = packages.dawn.nativeBuildInputs ++ [
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
