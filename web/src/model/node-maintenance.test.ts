import { expect, test } from "bun:test"

import { kubectlCommand } from "@/model/node-maintenance"

test("the kubectl command of a plan", () => {
  expect(kubectlCommand("cordon", "vps-3")).toBe("kubectl cordon vps-3")
  expect(kubectlCommand("uncordon", "vps-3")).toBe("kubectl uncordon vps-3")
})
