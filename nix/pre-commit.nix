# https://pre-commit.com/ hooks defined in Nix
# cf. https://github.com/cachix/pre-commit-hooks.nix
{ ... }:

{
  perSystem = { pkgs, lib, ... }: {
    pre-commit = {
      # clippy check doesn't work in pure Nix builds
      # cf. https://github.com/cachix/pre-commit-hooks.nix/issues/94
      # Also, treefmt provides its own checks.
      # Ergo, disable checks.
      check.enable = false;
      settings = {
        hooks = {
          treefmt.enable = true;

          # Custom hooks
          clippy-nix = {
            enable = true;
            name = "clippy";
            description = "Run clippy as defined in rust.nix";
            files = "\\.rs$";
            entry = "cargo clippy --all-targets --all-features -- --deny warnings";
            pass_filenames = false;
          };

          trailing-ws = {
            enable = true;
            name = "trailing-ws";
            description = "Remove trailing spaces";
            types = [ "text" ];
            # files = "crates/.*$";
            pass_filenames = true;
            entry = lib.getExe (pkgs.writeShellApplication {
              name = "trailing-ws";
              text = ''
                if [[ ''$# -gt 0 ]]; then
                  echo "Checking for trailing spaces in ''$# files"
                  FILES_WITH_TRAILING_WS=$(grep -r -l '  *$' "''$@" || true)
                  if [ -z "$FILES_WITH_TRAILING_WS" ]; then
                    echo "No trailing spaces found"
                  else
                    echo "Trailing spaces found in:"
                    echo "''$FILES_WITH_TRAILING_WS"

                    echo "Removing trailing spaces, please check and git add the changes"
                    if [[ "''$OSTYPE" == 'darwin'* ]]; then
                      BACKUP_EXTENSION=pqmc98hxvymotiyb4rz34
                      sed -i".''${BACKUP_EXTENSION}" -e 's/  *$//' "''$FILES_WITH_TRAILING_WS"
                      find ./ -name "*.''${BACKUP_EXTENSION}" -exec rm {} +
                    else
                      sed -i -e 's/  *$//' "''$FILES_WITH_TRAILING_WS"
                    fi
                  fi
                fi
              '';
            });
          };
        };
      };
    };
  };
}
