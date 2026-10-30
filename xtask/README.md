# xtask

Development tasks for this repository, in the [cargo xtask](https://github.com/matklad/cargo-xtask)
convention: `cargo xtask <task>` (an alias in `.cargo/config.toml`) builds and runs this crate.
Nothing ships it. Git, Docker, k3d, kubectl, Helm, openssl, curl and Nix run as subprocesses;
`cargo xtask --help` prints every task, and `cargo xtask <task> --help` its options.

```sh
# The development cluster (scripts/dev-cluster.sh, `nix run .#dev-cluster`): a k3d cluster with
# Zeabur Identity and Zeabur at https://zeabur.localhost. A second `up` brings a running cluster
# in line with the checkout. State is in target/dev-cluster.
cargo xtask dev-cluster up [--profile bare|default|full] [--identity-src <dir>]
                           [--builder-src <dir>] [--zeabur-image <image>] [--zeabur-chart <chart>]
cargo xtask dev-cluster down
cargo xtask dev-cluster snapshot   # cluster-scoped objects and namespace metadata, for e2e-l2

# Zeabur Proxy's canary (scripts/e2e-proxy.sh) on the cluster KUBECONFIG points at: no person
# reads another namespace's log lines or series. Removes what it made, however it ends, unless
# ZEABUR_PROXY_E2E_KEEP=1.
cargo xtask e2e-proxy

# A checkout of a pinned commit or release tag (vX.Y.Z) from <url> in <dir>, kept between runs
# for its build cache; untracked files other than target/ are removed.
cargo xtask dev-cluster checkout <dir> <pin> <url>
```
