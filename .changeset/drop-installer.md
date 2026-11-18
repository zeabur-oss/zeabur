---
default: minor
---

# Remove zeabur-installer; the chart guards system hosts

`zeabur-installer` and its release archives are gone. To run Zeabur, install the chart on an existing Kubernetes cluster. Setting up a server from nothing moves to Zeatend ([#430](https://github.com/zeabur-oss/zeabur/issues/430)). The cluster page no longer shows installer steps for upgrades, and "How to install" on a missing component prints the `zeaburctl components export` command.

The chart now has the `zeabur-system-hosts` ValidatingAdmissionPolicy that the installer used to apply: an Ingress or HTTPRoute in a project cannot claim the host of Zeabur or Zeabur Identity. It reads those hosts from the release's namespace and from each namespace in `systemHosts.namespaces` (default `zeabur-identity`). Set `systemHosts.enabled: false` to leave it out.
