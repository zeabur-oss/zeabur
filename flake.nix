{
  description = "zeabur development environment";

  inputs = {
    nixpkgs.url = "https://flakehub.com/f/NixOS/nixpkgs/0.1"; # unstable Nixpkgs
    fenix = {
      url = "https://flakehub.com/f/nix-community/fenix/0.1";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    # The zh-TW wording check (scripts/check-zh-tw.sh, `nix develop .#zh`). Upstream has no
    # release tags, so it is pinned to a commit; bump it here, to a commit at least seven days old
    # (the same delay as crates and web packages), then `nix flake lock`.
    zhtw-mcp = {
      url = "github:sysprog21/zhtw-mcp/5b385b3d66b1554f6f9ae036017ef06dd8ec0da6";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.fenix.follows = "fenix";
    };
    # Compiler cache, built from source at a release tag.
    kache = {
      url = "github:kunobi-ninja/kache/v1.2.2";
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
                inputs.kache.overlays.default
                inputs.self.overlays.default
              ];
            };
          }
        );
    in
    {
      overlays.default = final: prev: {
        # rust-toolchain.toml is the only place the Rust version is set.
        rustToolchain = inputs.fenix.packages.${prev.stdenv.hostPlatform.system}.fromToolchainFile {
          file = ./rust-toolchain.toml;
          sha256 = "sha256-ackdOtCWk+NYS4SXZE1LZolba9ZfGWmu+4GhiK2Fq9E=";
        };
        # kache's own CI tests each release. Its 4,500 tests take 6 min here and some are timing-sensitive.
        kache = prev.kache.overrideAttrs { doCheck = false; };
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
              kache
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
              # end-to-end test (scripts/e2e-l2.sh: a CA and certificates for its host names)
              kubernetes-helm
              yq-go
              openssl

              # Releases (.github/workflows/release.yml, scripts/release-*.sh): keyless signing,
              # copying image candidates to the registry, the draft release.
              cosign
              skopeo
              gh

              self.formatter.${system}
            ];

            env = {
              # Compiler cache shared by every checkout and worktree; release builds turn it off.
              RUSTC_WRAPPER = "kache";
              # Required by rust-analyzer
              RUST_SRC_PATH = "${pkgs.rustToolchain}/lib/rustlib/src/rust/library";
              # Mozilla's CA bundle (MPL-2.0), copied into the image for outgoing TLS (Zeabur Identity's
              # discovery and token endpoints); pinned with nixpkgs.
              ZEABUR_CA_BUNDLE = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";
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

      formatter = forEachSupportedSystem ({ pkgs, ... }: pkgs.nixfmt);
    };
}
