{
  description = "poster";

  nixConfig = {
    extra-substituters = [
      "https://devcache.lautaroacosta.com/dev-cache"
    ];

    extra-trusted-public-keys = [
      "dev-cache:lV4Nej+9+n3g2uAgGKIBpaffUvzKR1essUx1opU33/0="
    ];
  };

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-26.05";

    flake-parts.url = "github:hercules-ci/flake-parts";

    crane.url = "github:ipetkov/crane";

    advisory-db = {
      url = "github:rustsec/advisory-db";
      flake = false;
    };
  };

  outputs =
    inputs@{
      flake-parts,
      crane,
      advisory-db,
      ...
    }:
    flake-parts.lib.mkFlake { inherit inputs; } {

      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];

      perSystem =
        { pkgs, config, ... }:
        let
          craneLib = crane.mkLib pkgs;

          src = pkgs.lib.fileset.toSource {
            root = ./.;
            fileset = pkgs.lib.fileset.unions [
              (craneLib.fileset.commonCargoSources ./.)
              (pkgs.lib.fileset.maybeMissing ./crates/poster/migrations)
              (pkgs.lib.fileset.maybeMissing ./crates/poster/static)
              (pkgs.lib.fileset.maybeMissing ./.sqlx)
              (pkgs.lib.fileset.maybeMissing ./configuration)
            ];
          };

          commonArgs = {
            inherit src;
            cargoExtraArgs = "--package poster --bin poster";
            strictDeps = true;
            env.SQLX_OFFLINE = true;
            nativeBuildInputs = [ pkgs.cacert ];
            env.NIX_SSL_CERT_FILE = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";
          };

          cargoArtifacts = craneLib.buildDepsOnly commonArgs;

          app = craneLib.buildPackage (
            commonArgs
            // {
              inherit cargoArtifacts;
              doCheck = false;
            }
          );

          imageRoot = pkgs.runCommand "image-root" { } ''
            mkdir -p $out/bin
            ln -s ${app}/bin/poster $out/bin/poster

            # Runtime assets served by the app (ServeDir "static")
            if [ -d ${src}/static ]; then
              mkdir -p $out/static
              cp -r ${src}/static/. $out/static/
            fi

            # Optional config files (required by the app unless overridden via env)
            if [ -d ${src}/configuration ]; then
              mkdir -p $out/configuration
              cp -r ${src}/configuration/. $out/configuration/
            fi
          '';

          dockerImage = pkgs.dockerTools.buildImage {
            name = "poster";
            tag = "latest";

            copyToRoot = imageRoot;

            config = {
              Cmd = [
                "/bin/poster"
              ];
              WorkingDir = "/";
              Env = [
                "APP_ENVIRONMENT=prod"
              ];
            };
          };

        in
        {
          packages = {
            default = app;
            dependencies = cargoArtifacts;
            docker = dockerImage;
          };

          checks = {
            package = app;

            machete =
              pkgs.runCommand "cargo-machete"
                {
                  inherit src;
                  nativeBuildInputs = [ pkgs.cargo-machete ];
                }
                ''
                  cd $src
                  cargo-machete
                  touch $out
                '';

            audit = craneLib.cargoAudit {
              inherit src advisory-db;
              cargoAuditExtraArgs = "--ignore yanked --ignore RUSTSEC-2026-0235";
            };

            deny = craneLib.cargoDeny {
              inherit src;
            };
          };

          devShells.default = pkgs.mkShell {
            packages =
              with pkgs;
              [
                cargo-nextest
                cargo-llvm-cov
                cargo-semver-checks
                cargo-audit
                cargo-machete
                cargo-deny
                cargo-insta
                rustc
                cargo
                rustfmt
                clippy
                rust-analyzer
                llvmPackages.llvm
                pkg-config
                just
                sqlx-cli
                sccache
                dive
              ]
              ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
                pkgs.libiconv
              ];

            RUST_BACKTRACE = 1;
            RUST_LOG = "debug";

            shellHook = ''
              export RUSTC_WRAPPER="${pkgs.sccache}/bin/sccache"
              export CARGO_HOME="$PWD/.cargo"
              export LLVM_COV="${pkgs.llvmPackages.llvm}/bin/llvm-cov"
              export LLVM_PROFDATA="${pkgs.llvmPackages.llvm}/bin/llvm-profdata"
            '';
          };
        };
    };
}
