{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    systems.url = "github:nix-systems/default";

    # Rust
    dream2nix.url = "github:nix-community/dream2nix";

    # Dev tools
    treefmt-nix.url = "github:numtide/treefmt-nix";
    mission-control.url = "github:Platonic-Systems/mission-control";
    flake-root.url = "github:srid/flake-root";

    # Services
    process-compose-flake.url = "github:Platonic-Systems/process-compose-flake";
    # TODO: update flake after https://github.com/juspay/services-flake/pull/8 is merged.
    services-flake.url = "github:juspay/services-flake/redis/init";
  };
  outputs = inputs:
    inputs.flake-parts.lib.mkFlake { inherit inputs; } {
      systems = import inputs.systems;
      imports = [
        inputs.dream2nix.flakeModuleBeta
        inputs.treefmt-nix.flakeModule
        inputs.mission-control.flakeModule
        inputs.flake-root.flakeModule
        inputs.process-compose-flake.flakeModule
      ];
      perSystem = { config, self', pkgs, lib, system, ... }: {
        process-compose."services" = {
          imports = [
            inputs.services-flake.processComposeModules.default
          ];
          services.redis = {
            enable = true;
          };
        };
        # Rust project definition
        # cf. https://github.com/nix-community/dream2nix
        dream2nix.inputs."location-tracking-service" = {
          source = lib.sourceFilesBySuffices ./. [
            ".rs"
            "Cargo.toml"
            "Cargo.lock"
          ];
          projects.location-tracking-service = {
            name = "location-tracking-service";
            subsystem = "rust";
            translator = "cargo-lock";
          };
          packageOverrides = rec {
            location-tracking-service = {
              add-deps = with pkgs; with pkgs.darwin.apple_sdk.frameworks; {
                nativeBuildInputs = old: old ++ lib.optionals stdenv.isDarwin [
                  Security
                ] ++ [
                  libiconv
                  pkg-config
                  pkg-config
                ];
              };
            };
            location-tracking-service-deps = location-tracking-service;
          };
        };

        # Flake outputs
        packages = config.dream2nix.outputs.location-tracking-service.packages;
        devShells.default = pkgs.mkShell {
          inputsFrom = [
            config.dream2nix.outputs.location-tracking-service.devShells.default
            config.treefmt.build.devShell
            config.mission-control.devShell
            config.flake-root.devShell
          ];
          shellHook = ''
            # For rust-analyzer 'hover' tooltips to work.
            export RUST_SRC_PATH=${pkgs.rustPlatform.rustLibSrc}
            export REDIS_HOST=${config.process-compose.services.services.redis.bind}
            export LOCATION_EXPIRY="90"
            export TOKEN_EXPIRY="30"
            export ON_RIDE_EXPIRY="172800"
          '';
          nativeBuildInputs = [
            # Add your dev tools here.
            pkgs.cargo-watch
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

        # Makefile'esque but in Nix. Add your dev scripts here.
        # cf. https://github.com/Platonic-Systems/mission-control
        mission-control.scripts = {
          fmt = {
            exec = config.treefmt.build.wrapper;
            description = "Auto-format project tree";
          };

          run = {
            exec = ''cargo run'';
            description = "Run the project executable";
          };

          watch = {
            exec = ''cargo watch -x run'';
            description = "Watch for changes and run the project executable";
          };

          services = {
            exec = self'.packages.services;
            description = "Run the project service dependencies";
          };
        };
      };
    };
}
