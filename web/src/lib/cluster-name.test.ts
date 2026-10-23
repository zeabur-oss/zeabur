import { describe, expect, test } from "bun:test"

import type { Schemas } from "@/api/client"
import { translator } from "@/i18n"
import { en } from "@/i18n/en"
import { nameSourceOf } from "@/lib/cluster-name"

const t = translator(en)
const cluster = (part: Partial<Schemas["Cluster"]>): Schemas["Cluster"] => ({
  name: "zeabur-l2",
  nameSource: "nodes",
  kubernetesVersion: "v1.36.5+k3s1",
  domains: {},
  builds: {},
  can: { installComponents: "allowed", changeNodes: "allowed" },
  ...part,
})

describe("where the cluster's name comes from", () => {
  test("a provider by its own name, a distribution by its node", () => {
    expect(nameSourceOf(cluster({ nameProvider: "k3d" }), t)).toBe("Name from k3d")
    expect(nameSourceOf(cluster({ nameProvider: "docker-desktop" }), t)).toBe(
      "Name from Docker Desktop",
    )
    expect(nameSourceOf(cluster({ nameProvider: "rke2" }), t)).toBe(
      "Name from the RKE2 control plane node",
    )
    expect(nameSourceOf(cluster({ nameProvider: "node" }), t)).toBe(
      "Name from the control plane node",
    )
  })

  test("a name someone gave, or the address", () => {
    expect(nameSourceOf(cluster({ nameSource: "setting" }), t)).toBe("Name from Settings")
    expect(nameSourceOf(cluster({ nameSource: "install" }), t)).toBe("Name from the installation")
    expect(nameSourceOf(cluster({ nameSource: "address" }), t)).toBe("Name from the URL")
    expect(nameSourceOf(cluster({ nameSource: "kubeconfig", nameProvider: "k3d" }), t)).toBe(
      "Name from kubeconfig",
    )
  })
})
