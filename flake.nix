{
  description = "zeabur development environment";

  inputs = {
    nixpkgs.url = "https://flakehub.com/f/NixOS/nixpkgs/0.1"; # unstable Nixpkgs
    # Rust toolchains. The locked revision sets the nightly (overlays.default).
    rust-overlay = {
      url = "https://flakehub.com/f/oxalica/rust-overlay/0.1";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    # The zh-TW wording check (scripts/check-zh-tw.sh, `nix develop .#zh`). Upstream has no
    # release tags, so it is pinned to a commit; bump it here, to a commit at least seven days old
    # (the same delay as crates and web packages), then `nix flake lock`.
    zhtw-mcp = {
      url = "github:sysprog21/zhtw-mcp/5b385b3d66b1554f6f9ae036017ef06dd8ec0da6";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    # Zeabur Catalog. flake.lock pins its commit: zeabur-catalog embeds the bundle that the
    # catalog's `release` output builds there, and a release build checks that bundle against the
    # signed release at that commit (scripts/release-pins.sh).
    catalog.url = "github:zeabur-oss/catalog";
    # zeabur-oss-release (scripts/release.sh) and zeabur-oss-unsafe-report (scripts/ci.sh).
    oss-release = {
      url = "github:zeabur-oss/.github";
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
      overlays.default = final: _: {
        # The newest nightly in the locked rust-overlay that has all these components and targets.
        rustToolchain = final.rust-bin.selectLatestNightlyWith (
          toolchain:
          toolchain.minimal.override {
            extensions = [
              "rustfmt"
              "clippy"
              "rust-src"
              "rust-analyzer"
            ];
            targets = [
              "x86_64-unknown-linux-musl"
              "aarch64-unknown-linux-musl"
            ];
          }
        );
      };

      devShells = forEachSupportedSystem (
        { pkgs, system }:
        {
          default = pkgs.mkShell {
            packages = with pkgs; [
              rustToolchain

              # Quality and supply chain
              cargo-deny
              cargo-nextest
              cargo-auditable
              cargo-cyclonedx
              # Mutation testing, weekly in CI (.github/workflows/mutants.yml); a report, not a gate
              cargo-mutants
              # Zeabur Proxy's parser fuzz target (crates/zeabur-proxy/fuzz)
              cargo-fuzz
              jq
              shellcheck
              actionlint

              # Web interface (web/: dependencies come from bun.lock)
              bun
              brotli
              # Documentation screenshots (scripts/docs-shots.sh)
              libavif

              # Kubernetes integration tests
              k3d
              kubectl

              # The Helm chart (deploy/chart/zeabur, scripts/check-chart.sh) and the in-cluster
              # development cluster (scripts/dev-cluster.sh: a CA and certificates for its host names)
              kubernetes-helm
              yq-go
              openssl

              # Releases (scripts/release.sh, scripts/release-*.sh)
              inputs.oss-release.packages.${system}.zeabur-oss-release
              inputs.oss-release.packages.${system}.zeabur-oss-unsafe-report
              cosign
              skopeo
              gh

              self.formatter.${system}
            ];

            env = {
              # Required by rust-analyzer
              RUST_SRC_PATH = "${pkgs.rustToolchain}/lib/rustlib/src/rust/library";
              # Mozilla's CA bundle (MPL-2.0), copied into the image for outgoing TLS (Zeabur Identity's
              # discovery and token endpoints); pinned with nixpkgs.
              ZEABUR_CA_BUNDLE = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";
              # Zeabur Catalog at the commit in flake.lock: its source (scripts/render-chart-corpus.sh)
              # and its release files, bundle.tar.gz, index.json and checksums.txt (zeabur-catalog).
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

          # Only for the zh-TW wording check: zhtw-mcp builds from source, so it stays out of the
          # default shell that every other CI job enters.
          zh = pkgs.mkShell {
            packages = [
              pkgs.bun
              inputs.zhtw-mcp.packages.${system}.zhtw-mcp
            ];
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
