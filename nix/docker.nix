{ self, ... }:

{
  perSystem = { self', pkgs, lib, system, ... }:
    let
      imageNameSuffix = {
        "aarch64-linux" = "-arm64";
        "aarch64-darwin" = "-arm64";
      };
      imageName = "ghcr.io/nammayatri/rust-microservices" + (imageNameSuffix.${system} or "");
      # self.rev will be non-null only when the working tree is clean
      # This is equivalent to `git rev-parse --short HEAD`
      imageTag = builtins.substring 0 6 (self.rev or "dev");
    in
    {
      packages = lib.optionalAttrs pkgs.stdenv.isLinux {
        dockerImage = pkgs.dockerTools.buildImage {
          name = imageName;
          created = "now";
          tag = imageTag;
          copyToRoot = pkgs.buildEnv {
            paths = with pkgs; [
              cacert
              awscli
              coreutils
              bash
              self'.packages.default
            ];
            name = "rust-microservices";
            pathsToLink = [
              "/bin"
              "/opt"
            ];
          };
          config = {
            Env = [
              "SSL_CERT_FILE=${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt"
            ];
            Cmd = [ "${lib.getExe self'.packages.default}" ];
          };

          # Test that the docker image contains contents we expected for
          # production.
          extraCommands = ''
          '';
        };
      };
    };
}
