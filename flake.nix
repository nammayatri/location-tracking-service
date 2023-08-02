{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    systems.url = "github:nix-systems/default";

    # Rust
    rust-overlay.url = "github:oxalica/rust-overlay";
    crane.url = "github:ipetkov/crane";
    crane.inputs.nixpkgs.follows = "nixpkgs";

    # Dev tools
    treefmt-nix.url = "github:numtide/treefmt-nix";

    # Services
    process-compose-flake.url = "github:Platonic-Systems/process-compose-flake";
    services-flake.url = "github:juspay/services-flake";
  };
  outputs = inputs:
    inputs.flake-parts.lib.mkFlake { inherit inputs; } {
      systems = import inputs.systems;
      imports = [
        inputs.treefmt-nix.flakeModule
        inputs.process-compose-flake.flakeModule
      ];
      perSystem = { config, self', pkgs, lib, system, ... }:
        let
          rustToolchain = (pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml).override {
            extensions = [
              "rust-src"
              "rust-analyzer"
            ];
          };
          craneLib = (inputs.crane.mkLib pkgs).overrideToolchain rustToolchain;
          package = craneLib.buildPackage {
            pname = "location-tracking-service";
            src = ./.;
            doCheck = false; # FIXME: tests require services to be running
            buildInputs = lib.optionals pkgs.stdenv.isDarwin
              (with pkgs.darwin.apple_sdk.frameworks; [
                Security
              ]) ++ [
              pkgs.libiconv
              pkgs.openssl
            ];
            nativeBuildInputs = [ pkgs.pkg-config ];
          };
        in
        {
          _module.args.pkgs = import inputs.nixpkgs {
            inherit system;
            overlays = [
              inputs.rust-overlay.overlays.default
            ];
          };

          process-compose."lts-services" = {
            imports = [
              inputs.services-flake.processComposeModules.default
            ];
            services.redis."redis1" = {
              enable = true;
            };
          };

          # Flake outputs
          packages.default = package;
          devShells.default = pkgs.mkShell {
            inputsFrom = [
              config.treefmt.build.devShell
              self'.packages.default # Makes the buildInputs of the package available in devShell (so cargo can link against Nix libraries)
            ];
            shellHook = ''
              # For rust-analyzer 'hover' tooltips to work.
              export RUST_SRC_PATH="${rustToolchain}/lib/rustlib/src/rust/library";

              export REDIS_HOST=${config.process-compose."lts-services".services.redis."redis1".bind}
              export DATABASE_URL=postgresql://postgres:root@localhost:5434/atlas_dev
              export AUTH_URL=http://127.0.0.1:8016/internal/auth
              export TEST_LOC_EXPIRY="90"
              export LOCATION_EXPIRY="60"
              export TOKEN_EXPIRY="30"
              export ON_RIDE_EXPIRY="172800"

              echo
              echo "🍎🍎 Run 'just <recipe>' to get started"
              just
            '';
            nativeBuildInputs = with pkgs; [
              # Add your dev tools here.
              cargo
              rustc
              rust-analyzer
              cargo-watch
              just
              # Programs used by `justfile`
              config.process-compose."lts-services".outputs.package
            ];
          };

          # Add your auto-formatters here.
          # cf. https://numtide.github.io/treefmt/
          treefmt.config = {
            projectRootFile = "flake.nix";
            programs = {
              nixpkgs-fmt.enable = true;
              rustfmt.enable = true;
            };
          };
        };
    };
}
