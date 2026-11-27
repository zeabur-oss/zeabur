{
  description = "zeabur development environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    # Rust toolchains. rust-toolchain.toml sets the nightly; the locked revision must have its manifest.
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    # The zh-TW wording check (scripts/check-zh-tw.sh, `nix develop .#zh`). Upstream has no
    # release tags, so it is pinned to a commit; bump it here, to a commit at least seven days old
    # (the same delay as crates and web packages), then `nix flake lock`.
    zhtw-mcp = {
      url = "github:sysprog21/zhtw-mcp/37995816d042b58b3670a63fdd673cd556a2ce20";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    # Zeabur Catalog at a release tag, the same tag as zeabur-catalog-format in Cargo.toml (a test
    # in zeabur-catalog checks it). zeabur-catalog embeds the bundle that the catalog's `release`
    # output builds there, and a release build checks that bundle against the signed release at
    # that commit (scripts/release-pins.sh).
    catalog = {
      url = "github:zeabur-oss/catalog/v0.6.3";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.oss.follows = "oss";
    };
    # The base shell of every zeabur-oss repository (Bun, the release tools and the shared linters).
    oss = {
      url = "github:zeabur-oss/.github/395adf27d31c77538ebf7c154946b52822387b96";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    { self, ... }@inputs:

    let
      supportedSystems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];
      forEachSupportedSystem =
        f:
        inputs.nixpkgs.lib.genAttrs supportedSystems (
          system:
          f {
            inherit system;
            pkgs = import inputs.nixpkgs {
              inherit system;
              overlays = [
                inputs.rust-overlay.overlays.default
                inputs.self.overlays.default
              ];
            };
          }
        );
    in
    {
      overlays.default = final: prev: {
        # The nightly, components and targets of rust-toolchain.toml, the same file rustup reads.
        rustToolchain = final.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
        # Nixpkgs has 2.12.4, whose RPM bundle refuses a pre-release version such as 0.3.0-beta.0
        # (tauri-apps/tauri#16336, fixed in 2.12.5). Drop this once nixpkgs-unstable has 2.12.5.
        cargo-tauri = prev.cargo-tauri.overrideAttrs (
          finalAttrs: _: {
            version = "2.12.6";
            src = final.fetchFromGitHub {
              owner = "tauri-apps";
              repo = "tauri";
              tag = "tauri-cli-v${finalAttrs.version}";
              hash = "sha256-GPmPwKloggoRL38BS1GVgkzsSG95zgmGPcJFnstR63s=";
            };
            cargoDeps = final.rustPlatform.fetchCargoVendor {
              inherit (finalAttrs) pname version src;
              hash = "sha256-gJyKD3SZUwv/sk3VFr9wkjzcaSkm0xV3mwzIBhrS8hY=";
            };
          }
        );
      };

      devShells = forEachSupportedSystem (
        { pkgs, system }:
        {
          default = pkgs.mkShell {
            inputsFrom = [ inputs.oss.devShells.${system}.base ];
            packages = with pkgs; [
              rustToolchain
              # The separate CloudNativePG adapter imports Keel's Go packages.
              go

              # Quality and supply chain
              cargo-deny
              cargo-nextest
              cargo-auditable
              cargo-cyclonedx
              # Pattern rules (scripts/ci/patterns.sh)
              ripgrep
              # Zeabur Proxy's parser fuzz target (crates/zeabur-proxy/fuzz)
              cargo-fuzz
              # Breaking changes to crates/zeabur-api/openapi.yaml (scripts/api-break.ts)
              oasdiff
              # Web interfaces (the Bun workspace: dependencies come from the root bun.lock)
              brotli
              # Documentation screenshots (scripts/docs-shots.sh)
              libavif

              # Kubernetes integration tests
              k3d
              kubectl

              # The in-cluster development cluster (scripts/dev-cluster.sh: a CA and certificates for
              # its host names)
              openssl

              # Zeabur's alert rules (crates/zeabur-catalog/src/zeabur.rs): vmalert-tool
              victoriametrics
            ];

            env = {
              # Required by rust-analyzer
              RUST_SRC_PATH = "${pkgs.rustToolchain}/lib/rustlib/src/rust/library";
              # Mozilla's CA bundle (MPL-2.0), copied into the image for outgoing TLS (Zeabur Identity's
              # discovery and token endpoints); pinned with nixpkgs.
              ZEABUR_CA_BUNDLE = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";
              # Nix's Go output omits the license; package it from the pinned compiler source.
              ZEABUR_GO_LICENSE = pkgs.runCommand "go-${pkgs.go.version}-LICENSE" { } ''
                tar -xOf ${pkgs.go.src} go/LICENSE > "$out"
              '';
              # Zeabur Catalog at the commit in flake.lock: its source (scripts/render-chart-corpus.sh)
              # and its release file, bundle.tar.gz (zeabur-catalog).
              ZEABUR_CATALOG = "${inputs.catalog}";
              ZEABUR_CATALOG_COMMIT = inputs.catalog.rev or "dirty";
              ZEABUR_CATALOG_RELEASE = "${inputs.catalog.packages.${system}.release}";
              # Browser tests (scripts/e2e-ui.sh) and screenshots (scripts/docs-shots.sh): Chromium
              # from nixpkgs, matching the @playwright/test version pinned in web/package.json
              # (playwright-driver.version).
              PLAYWRIGHT_BROWSERS_PATH = "${pkgs.playwright-driver.browsers-chromium}";
            }
            // pkgs.lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux (
              # Static musl builds of the host architecture: aws-lc-sys must be compiled against musl
              # headers, not glibc ones (else `__isoc23_strtol`, `__memcpy_chk`... are undefined).
              let
                muslCc = pkgs.pkgsStatic.stdenv.cc;
                bin = "${muslCc}/bin/${muslCc.targetPrefix}";
                triple = "${pkgs.stdenv.hostPlatform.parsed.cpu.name}_unknown_linux_musl";
              in
              {
                "CC_${triple}" = "${bin}cc";
                "AR_${triple}" = "${bin}ar";
                "CARGO_TARGET_${pkgs.lib.toUpper triple}_LINKER" = "${bin}cc";
                PLAYWRIGHT_SKIP_VALIDATE_HOST_REQUIREMENTS = "true";
                # Chromium's fonts. nixpkgs gives it none and a Linux runner has no Chinese font, so
                # zh-TW and zh-CN text would show as boxes. Noto Sans CJK (OFL-1.1) is the Linux font
                # the design system names for Chinese; on macOS Chromium uses PingFang.
                FONTCONFIG_FILE = pkgs.makeFontsConf { fontDirectories = [ pkgs.noto-fonts-cjk-sans ]; };
              }
            );
          };

          # The desktop app (desktop/, scripts/ci/desktop.sh): the default shell, the Tauri CLI
          # that bundles the app (`cargo tauri build`), and on Linux the GTK 3 and WebKitGTK
          # libraries that Tauri links. They stay out of the default shell that every other CI job
          # enters.
          desktop = self.devShells.${system}.default.overrideAttrs (old: {
            nativeBuildInputs =
              old.nativeBuildInputs
              ++ [ pkgs.cargo-tauri ]
              ++ pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux [ pkgs.pkg-config ];
            buildInputs =
              old.buildInputs
              ++ pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux (
                with pkgs;
                [
                  webkitgtk_4_1
                  gtk3
                  libsoup_3
                  dbus
                  libx11
                ]
              );
          });

          # Only for the zh-TW wording check: zhtw-mcp builds from source, so it stays out of the
          # default shell that every other CI job enters.
          zh = pkgs.mkShell {
            inputsFrom = [ inputs.oss.devShells.${system}.base ];
            packages = [ inputs.zhtw-mcp.packages.${system}.zhtw-mcp ];
          };
        }
      );

      # zhtw-mcp on its own, so CI can key a cache on its derivation (`.#zhtw-mcp.drvPath`).
      packages = forEachSupportedSystem (
        { system, ... }: { inherit (inputs.zhtw-mcp.packages.${system}) zhtw-mcp; }
      );

      # `nix run .#dev-cluster -- up`: scripts/dev-cluster.sh of the checkout it runs in, not of
      # the flake's copy in the store.
      apps = forEachSupportedSystem (
        { pkgs, ... }:
        {
          dev-cluster = {
            type = "app";
            meta.description = "A k3d cluster with Zeabur Identity and Zeabur";
            program = pkgs.lib.getExe (
              pkgs.writeShellApplication {
                name = "dev-cluster";
                runtimeInputs = [ pkgs.git ];
                text = ''exec "$(git rev-parse --show-toplevel)/scripts/dev-cluster.sh" "$@"'';
              }
            );
          };
        }
      );

      formatter = forEachSupportedSystem ({ pkgs, ... }: pkgs.nixfmt);
    };
}
