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

          add-gpl-header = {
            enable = true;
            name = "add-gpl-header";
            description = "Add GNU GPL license header to Rust source files";
            files = "\\.rs$";
            pass_filenames = true;
            entry = lib.getExe (pkgs.writeShellApplication {
              name = "add-gpl-header";
              text = ''
                LICENSE="/*  Copyright 2022-23, Juspay India Pvt Ltd
                    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
                    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
                    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
                    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
                    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
                */"
                if [[ ''$# -gt 0 ]]; then
                  echo "Checking for GNU GPL license in ''$# files"
                  FILES_NOT_WITH_LICENSE=$(grep -r -LF "''$LICENSE" "''$@" || true)
                  if [ -z "''$FILES_NOT_WITH_LICENSE" ]; then
                    echo "No file without license found"
                  else
                    echo "No license found in:"
                    echo "''$FILES_NOT_WITH_LICENSE"

                    echo "Adding license, please check and git add the changes"
                    for FILE in ''$FILES_NOT_WITH_LICENSE; do
                      printf "%s\n\n%s" "''$LICENSE" "''$(cat "''$FILE")" > "''$FILE"
                    done
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
