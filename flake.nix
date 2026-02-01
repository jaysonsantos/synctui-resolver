{
  description = "synctui-resolver (Syncthing conflict resolver TUI)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    crane.url = "github:ipetkov/crane";
  };

  outputs = {
    self,
    nixpkgs,
    flake-utils,
    crane,
    ...
  }:
    flake-utils.lib.eachDefaultSystem (system: let
      pkgs = import nixpkgs {
        inherit system;
      };

      craneLib = crane.lib.${system};

      src = craneLib.cleanCargoSource ./.;

      commonArgs = {
        inherit src;

        pname = "synctui-resolver";
        version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).package.version;

        strictDeps = true;

        buildInputs =
          pkgs.lib.optionals pkgs.stdenv.hostPlatform.isDarwin [
            pkgs.libiconv
            pkgs.darwin.apple_sdk.frameworks.Security
            pkgs.darwin.apple_sdk.frameworks.SystemConfiguration
          ];
      };

      cargoArtifacts = craneLib.buildDepsOnly commonArgs;

      synctui-resolver = craneLib.buildPackage (
        commonArgs
        // {
          inherit cargoArtifacts;
          doCheck = true;
        }
      );
    in {
      packages.default = synctui-resolver;
      packages.synctui-resolver = synctui-resolver;

      apps.default = flake-utils.lib.mkApp {
        drv = synctui-resolver;
        exePath = "/bin/synctui-resolver";
      };

      devShells.default = pkgs.mkShell {
        packages = [
          pkgs.cargo
          pkgs.rustc
          pkgs.rustfmt
          pkgs.clippy
        ];
      };

      checks = {
        inherit synctui-resolver;
      };
    });
}
