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
          port = 6380;
        };
        services.redis."redis2" = {
          enable = true;
          port = 6381;
        };
        services.zookeeper."zookeeper1".enable = true;
        services.apache-kafka."kafka1".enable = true;
      };

      # Flake outputs
      devShells.services = pkgs.mkShell {
        nativeBuildInputs = [
          config.process-compose."lts-services".outputs.package
        ];
      };
    };
}
