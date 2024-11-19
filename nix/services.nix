# Services and processes required by this project
{ inputs, ... }: {
  perSystem = { config, self', pkgs, lib, system, ... }:

    {
      process-compose."lts-services" = pc: {
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
        services.apache-kafka."kafka1" = {
          enable = true;
          settings = {
            "zookeeper.connect" = with pc.config.services.zookeeper; [ "localhost:${builtins.toString zookeeper1.port}" ];
          };
        };
        # kafka should start only after zookeeper is healthy
        settings.processes."kafka1".depends_on."zookeeper1".condition = "process_healthy";
      };

      # Flake outputs
      devShells.services = pkgs.mkShell {
        inputsFrom = [
          config.process-compose."lts-services".services.outputs.devShell
        ];
        packages = [
          config.process-compose."lts-services".outputs.package
        ];
      };
    };
}
