# engawa-recommend

Reads the recommendations catalogue and tells, for each capability (ingress, certificates,
backups, …), whether the cluster already has it; for a missing one it lists what could be
installed, and writes the install as a `helm` command, a Flux `HelmRelease`, an Argo CD
`Application` or a k3s `HelmChart` (`tests/golden/` holds each form for a few components). It
also says how long the cluster's Kubernetes version is supported and whether it should be upgraded
(`versions`).

## Hides

The catalogue format and the detection rules. engawa suggests; it never installs.

## Place in the workspace

Level 3: may depend on `engawa-core`, `engawa-model` and `engawa-kube` (for the facts it reads).

## Design

- **The catalogue is embedded.** `bundle/` is the
  [recommendations](https://github.com/engawahq/recommendations) catalogue at the commit in
  `bundle/PINNED` (Apache-2.0, `bundle/LICENSE`); `scripts/check-recommendations.sh` fails CI if a
  change makes it differ from that commit. The binary carries the catalogue it was built with and
  fetches nothing at run time. Once the catalogue publishes signed releases, the pin moves to a
  release.
- **Exists or missing.** A capability *exists* when any of its components' detection rules
  matches something in the cluster; otherwise it is missing and its components are what could be
  installed. engawa does not judge an existing installation against the catalogue's settings.
- **Recommended or optional.** A missing capability carries the catalogue's own default for a
  cluster of this shape: `recommended` (installed by default), `optional` (heavier or for
  specific needs) or `unsupported` (no component supports the distribution). The profile is
  `single-node` for one node, `multi-node` otherwise; the distribution comes from kube-apiserver's
  version string.
- **Evidence, not a verdict.** Every match is reported as a line of evidence (a CRD, an
  IngressClass controller, a k3s add-on, …), so a person can see why engawa thinks something is
  installed.
- **Secrets stay closed.** Helm charts are recognised by release name or by the `helm.sh/chart`
  label on workloads; release Secrets are listed by metadata only, never decoded.
- **Only for people who can see the whole cluster.** Detection runs with the person's rights,
  and a probe they may not run would find nothing, making an installed component look missing.
  So when any probe is refused, the API does not answer with capabilities at all (`403`), and the
  interface does not offer component management to that person.
