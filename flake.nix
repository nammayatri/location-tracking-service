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

    # Pre-commit
    pre-commit-hooks-nix.url = "github:cachix/pre-commit-hooks.nix";
  };
  outputs = inputs:
    inputs.flake-parts.lib.mkFlake { inherit inputs; } {
      systems = import inputs.systems;
      imports = [
        inputs.treefmt-nix.flakeModule
        inputs.process-compose-flake.flakeModule
        inputs.pre-commit-hooks-nix.flakeModule
        ./nix/rust.nix
        ./nix/services.nix
        ./nix/docker.nix
        ./nix/pre-commit.nix
      ];
      perSystem = { config, self', pkgs, lib, system, ... }:
        {
          _module.args.pkgs = import inputs.nixpkgs {
            inherit system;
            overlays = [
              inputs.rust-overlay.overlays.default
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

          # Flake outputs
          devShells.default = pkgs.mkShell {
            inputsFrom = [
              config.treefmt.build.devShell
              config.pre-commit.devShell
              self'.devShells.rust
              self'.devShells.services
            ];
            shellHook = ''
              export REDIS_HOST=${config.process-compose."lts-services".services.redis."redis1".bind}
              export DATABASE_URL=postgresql://postgres:root@localhost:5434/atlas_dev

              echo
              echo "🍎🍎 Run 'just <recipe>' to get started"
              just
            '';
            nativeBuildInputs = with pkgs; [
              just
            ];
          };

        };
    };
}
