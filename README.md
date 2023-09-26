# location-tracking-service

## Setting up development environment

### Nix

1. [Install **Nix**](https://github.com/DeterminateSystems/nix-installer#the-determinate-nix-installer)
    - If you already have Nix installed, you must [enable Flakes](https://nixos.wiki/wiki/Flakes#Enable_flakes) manually.
    - Then, run the following to check that everything is green ✅.
        ```sh
        nix run github:srid/nix-health
        ```
1. [Optional] Setup the Nix **binary cache**:
    ```sh
    nix run nixpkgs#cachix use nammayatri
    ```
    - For this command to succeed, you must have added yourself to the `trusted-users` list of `nix.conf`
1. Install **home-manager**[^hm] and setup **nix-direnv** and **starship** by following the instructions [in this home-manager template](https://github.com/juspay/nix-dev-home).[^direnv] [You want this](https://haskell.flake.page/direnv) to facilitate a nice Nix develoment environment.

[^hm]: Unless you are using NixOS in which case home-manager is not strictly needed.
[^direnv]: Not strictly required to develop the project. If you do not use `direnv` however you would have to remember to manually restart the `nix develop` shell, and know when exactly to do this each time.

### Rust

`cargo` is available in the Nix develop shell. You can also use one of the `just` commands (shown in Nix shell banner) to invoke cargo indirectly.

### VSCode

The necessary extensions are configured in `.vscode/`. See [nammayatri README](https://github.com/nammayatri/nammayatri/tree/main/Backend#visual-studio-code) for complete instructions.

### Autoformatting

Run `just fmt` (or `treefmt`) to auto-format the project tree. The CI checks for it.

### pre-commit

pre-commit hooks will be installed in the Nix devshell. Run the `pre-commit` command to manually run them. You can also run `pre-commit run -a` to run pre-commit hooks on *all* files (modified or not).

### Services

Run `just services` to run the service dependencies (example: redis-server) using [services-flake](https://github.com/juspay/services-flake).

## Usage / Installing

Run `nix build` in the project which produces a `./result` symlink. You can also run `nix run` to run the program immediately after build.

## Upstream Dependent Service

Clone and Run [dynamic-driver-offer-app](https://github.com/nammayatri/nammayatri) as it will be used for Internal Authentication and Testing postman flow.

## Postman Collection

Import the [Postman Collection](./Location%20Tracking%20Service%20Dev.postman_collection.json) in postman to test the API flow or Run `newman run LocationTrackingService.postman_collection.json --delay-request 2000`.

## Contributing PRs

Run `nix run nixpkgs#nixci` locally to make sure that the project builds. The CI runs the same.

## Profiling

In cargo.toml add :

```[profile.dev]
debug = true
debug-assertions = false

[profile.release]
debug = true
debug-assertions = false```
