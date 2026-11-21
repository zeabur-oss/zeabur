# Pins for the development cluster

The Zeabur Identity and Zeabur Builder releases that `nix run .#dev-cluster`, the cluster tests
(`scripts/ci/e2e.sh`, `scripts/ci/cluster.sh`) and the release smoke install. A Zeabur OSS release
does not ship or name them: people install Zeabur Identity and Zeabur Builder on their own.

| File | Holds | Used for |
|---|---|---|
| `zeabur-identity` | a release tag of zeabur-oss/identity | the signed release the cluster installs, unless `--identity-src` is given |
| `zeabur-builder` | a release tag or a full commit of zeabur-oss/builder | the source the `full` profile builds Zeabur Builder from, unless `--builder-src` is given |

Renovate moves both to newer release tags. Move one by hand in a pull request, then run the
cluster tests on it.
