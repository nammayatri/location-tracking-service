# Services and processes required by this project
{ inputs, ... }: {
  perSystem = { config, self', pkgs, lib, system, ... }:

    {
      process-compose."lts-services" = {
        imports = [
          inputs.services-flake.processComposeModules.default
        ];
        services.redis."redis1" = {
          enable = true;
        };
      };

      # Flake outputs
      devShells.services = pkgs.mkShell {
        nativeBuildInputs = [
          config.process-compose."lts-services".outputs.package
        ];
      };
    };
}
