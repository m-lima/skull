{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
    crane.url = "github:ipetkov/crane";
    fenix = {
      url = "github:nix-community/fenix";
      inputs = {
        nixpkgs.follows = "nixpkgs";
      };
    };
    flake-utils.url = "github:numtide/flake-utils";
    treefmt-nix = {
      url = "github:numtide/treefmt-nix";
      inputs = {
        nixpkgs.follows = "nixpkgs";
      };
    };
    helper.url = "github:m-lima/nix-template";
  };

  outputs =
    {
      nixpkgs,
      flake-utils,
      helper,
      ...
    }@inputs:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        sharedOptions = {
          allowFilesets = [
            ./store/.sqlx
            ./store/migrations
          ];
          systemLinker = pkgs.stdenv.isLinux;
          formatters = {
            beautysh.enable = true;
            prettier = {
              enable = true;
              settings = builtins.fromJSON (builtins.readFile ./web/.prettierrc.json);
            };
          };
          fmtExcludes = [
            "*.png"
            "*.sqlite"
            "*.txt"
            "*Dockerfile"
            ".dockerignore"
            "store/.sqlx/*.json"
            "store/migrations/*.sql"
          ];
          buildInputs = pkgs: [ pkgs.openssl ];
          nativeBuildInputs = pkgs: [ pkgs.pkg-config ];
          devPackages = pkgs: [
            (pkgs.writeShellScriptBin "sqlite" "exec ${pkgs.sqlite}/bin/sqlite3 -init ${pkgs.writeText "sqliteconfig" ".mode columns"} $@")
            pkgs.sqlx-cli
            pkgs.yarn
          ];
          overrides.commonArgs.LIBSQLITE3_FLAGS = "-DSQLITE_ENABLE_MATH_FUNCTIONS=1";
        };
        all = helper.lib.rust.helper inputs system ./. sharedOptions;
        server = helper.lib.rust.helper inputs system ./. (
          sharedOptions // { cargoExtraArgs = "-p server"; }
        );
        cli = helper.lib.rust.helper inputs system ./. (sharedOptions // { cargoExtraArgs = "-p cli"; });
        commonWeb =
          let
            package = builtins.fromJSON (builtins.readFile ./web/package.json);
          in
          {
            pname = package.name;
            version = package.version;

            nativeBuildInputs = [
              pkgs.nodejs
              pkgs.yarnConfigHook
              pkgs.yarnBuildHook
            ];

            src = pkgs.lib.fileset.toSource {
              root = ./web;
              fileset = pkgs.lib.fileset.unions [
                ./web/.prettierrc.json
                ./web/eslint.config.js
                ./web/index.html
                ./web/package.json
                ./web/public
                ./web/src
                ./web/tsconfig.json
                ./web/tsconfig.node.json
                ./web/vite.config.ts
                ./web/yarn.lock
              ];
            };

            offlineCache = pkgs.fetchYarnDeps {
              yarnLock = ./web/yarn.lock;
              hash = "sha256-BCDqJaWwb82JNws7GIehYL8dVBh4MpOr8DajGBGwfFE=";
            };

            doCheck = false;
          };
        web = pkgs.stdenvNoCC.mkDerivation (
          commonWeb
          // {
            installPhase = ''
              runHook preInstall
              mv dist $out
              runHook postInstall
            '';
          }
        );
      in
      all.outputs
      // {
        packages = {
          server = server.outputs.packages.default;
          cli = cli.outputs.packages.default;
          web = web;
        };
        checks = all.outputs.checks // {
          webCheck = pkgs.stdenvNoCC.mkDerivation (
            commonWeb
            // {
              doCheck = true;
              dontBuild = true;

              checkPhase = ''
                runHook preCheck
                yarn --offline lint:eslint
                yarn --offline lint:tsc
                runHook postCheck
              '';

              installPhase = "mkdir -p $out";
            }
          );
        };
        apps = {
          server = server.outputs.apps.default;
          cli = cli.outputs.apps.default;
        };
      }
    );
}
