# Nix for Rust project management
{ inputs, ... }: {
  perSystem = { config, self', pkgs, lib, system, ... }:
    let
      rustToolchain = (pkgs.rust-bin.fromRustupToolchainFile ../rust-toolchain.toml).override {
        extensions = [
          "rust-src"
          "rust-analyzer"
        ];
      };
      craneLib = (inputs.crane.mkLib pkgs).overrideToolchain rustToolchain;
      package = craneLib.buildPackage {
        pname = "location-tracking-service";
        src = ./..;
        doCheck = false; # FIXME: tests require services to be running
        buildInputs = lib.optionals pkgs.stdenv.isDarwin
          (with pkgs.darwin.apple_sdk.frameworks; [
            Security
          ]) ++ [
          pkgs.libiconv
          pkgs.openssl
          pkgs.rdkafka
        ];
        nativeBuildInputs = [
          pkgs.pkg-config
          pkgs.cmake
        ];
      };
    in
    {
      packages.default = package;

      # Flake outputs
      devShells.rust = pkgs.mkShell {
        inputsFrom = [
          package # Makes the buildInputs of the package available in devShell (so cargo can link against Nix libraries)
        ];
        shellHook = ''
          # For rust-analyzer 'hover' tooltips to work.
          export RUST_SRC_PATH="${rustToolchain}/lib/rustlib/src/rust/library";
        '';
        nativeBuildInputs = with pkgs; [
          # Add your dev tools here.
          cargo
          rustc
          rust-analyzer
          cargo-watch
        ];
      };
    };
}
