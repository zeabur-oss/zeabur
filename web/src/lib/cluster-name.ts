import type { Schemas } from "@/api/client"
import type { Translate } from "@/i18n"

type Cluster = Schemas["Cluster"]

const providers: Record<NonNullable<Cluster["nameProvider"]>, string> = {
  eks: "EKS",
  gke: "GKE",
  aks: "AKS",
  k3d: "k3d",
  kind: "kind",
  minikube: "minikube",
  "docker-desktop": "Docker Desktop",
  orbstack: "OrbStack",
  k3s: "k3s",
  k0s: "k0s",
  rke2: "RKE2",
  node: "",
}

/** Where the cluster's name comes from, in one short line. */
export function nameSourceOf(cluster: Cluster, t: Translate): string {
  switch (cluster.nameSource) {
    case "setting":
      return t("shell.nameFrom.setting")
    case "install":
      return t("shell.nameFrom.install")
    case "address":
      return t("shell.nameFrom.address")
    case "kubeconfig":
      return t("shell.nameFrom.kubeconfig")
    case "nodes": {
      const provider = cluster.nameProvider
      if (!provider || provider === "node") return t("shell.nameFrom.node")
      if (provider === "k3s" || provider === "k0s" || provider === "rke2")
        return t("shell.nameFrom.distributionNode", providers[provider])
      return t("shell.nameFrom.provider", providers[provider])
    }
  }
}
