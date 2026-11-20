# Pins for the development cluster

The Zeabur Identity release that `nix run .#dev-cluster`, the cluster tests
(`scripts/ci/e2e.sh`, `scripts/ci/cluster.sh`) and the release smoke install. A Zeabur release
does not ship or name it: people install Zeabur Identity on their own.

| File | Holds | Used for |
|---|---|---|
| `zeabur-identity` | a release tag of zeabur-oss/identity | the signed release the cluster installs, unless `--identity-src` is given |

Zeabur Builder has no pin here: the `full` profile builds the release that Zeabur Catalog
(`components/zeabur-builder.yaml`) installs, unless `--builder-src` is given. The
`zeabur-builder-api` tag in `Cargo.toml` must name the same release; a zeabur-catalog test checks it.

Renovate moves the Zeabur Identity pin to newer release tags. Move one by hand in a pull request, then run the
cluster tests on it.
