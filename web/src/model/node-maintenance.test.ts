import { describe, expect, test } from "bun:test"

import type { Schemas } from "@/api/client"
import {
  byProject,
  drain,
  FIRST_WAIT_MS,
  kubectlCommand,
  LONGEST_WAIT_MS,
  nextWait,
  settled,
} from "@/model/node-maintenance"

type Outcome = Schemas["EvictOutcome"]

const progress = (...pods: (Outcome | [Outcome, number])[]): Schemas["DrainProgress"] => {
  const list = pods.map((pod, i) => {
    const [outcome, retry] = Array.isArray(pod) ? pod : [pod, undefined]
    return { namespace: "shop", name: `p${i}`, outcome, retryAfterSeconds: retry }
  })
  const remaining = list.filter((p) => p.outcome !== "gone").length
  return { cordoned: true, pods: list, remaining, done: remaining === 0 }
}

describe("drain passes", () => {
  test("stop once nothing is left, or only refusals and failures are", () => {
    expect(settled(progress("gone", "gone"))).toBe(true)
    expect(settled(progress("gone", "refused", "failed"))).toBe(true)
    expect(settled(progress("gone", "leaving"))).toBe(false)
    expect(settled(progress("waiting"))).toBe(false)
    expect(settled(progress("gone", "last"))).toBe(false)
  })

  test("wait 2 s for replicas to stop, and back off while a budget holds one", () => {
    expect(nextWait(progress("leaving"), 16_000)).toBe(FIRST_WAIT_MS)
    expect(nextWait(progress("waiting"), undefined)).toBe(FIRST_WAIT_MS)
    expect(nextWait(progress("waiting"), 4_000)).toBe(8_000)
    expect(nextWait(progress("waiting"), 20_000)).toBe(LONGEST_WAIT_MS)
    expect(nextWait(progress(["waiting", 10]), 2_000)).toBe(10_000)
  })

  test("repeat until the node is empty, backing off on a budget", async () => {
    const answers = [
      progress(["waiting", 5], "leaving"),
      progress("waiting", "gone"),
      progress("leaving", "gone"),
      progress("gone", "gone"),
    ]
    const waits: number[] = []
    const seen: Schemas["DrainProgress"][] = []
    const last = await drain({
      first: progress("waiting", "leaving"),
      pass: async () => answers.shift() ?? progress("gone"),
      sleep: async (ms) => {
        waits.push(ms)
      },
      signal: new AbortController().signal,
      onProgress: (p) => seen.push(p),
    })
    expect(last.done).toBe(true)
    expect(seen).toHaveLength(4)
    expect(waits).toEqual([2_000, 5_000, 10_000, 2_000])
  })

  test("stop when the dialog closes", async () => {
    const abort = new AbortController()
    let passes = 0
    await drain({
      first: progress("leaving"),
      pass: async () => {
        passes += 1
        abort.abort()
        return progress("leaving")
      },
      sleep: async () => {},
      signal: abort.signal,
      onProgress: () => {},
    })
    expect(passes).toBe(1)
  })

  test("pass errors reach the caller", async () => {
    const run = drain({
      first: progress("leaving"),
      pass: async () => {
        throw new Error("409")
      },
      sleep: async () => {},
      signal: new AbortController().signal,
      onProgress: () => {},
    })
    expect(run).rejects.toThrow("409")
  })
})

describe("the plan", () => {
  const pod = (
    namespace: string,
    name: string,
    more: Partial<Schemas["DrainPod"]> = {},
  ): Schemas["DrainPod"] => ({
    namespace,
    name,
    fate: "moves",
    scratch: [],
    localClaims: [],
    budgetBlocks: false,
    zeabur: false,
    terminating: false,
    requests: { cpuMillicores: 0, memoryBytes: 0 },
    ...more,
  })

  test("groups pods by project, projects first, and counts each workload's replicas", () => {
    const groups = byProject([
      pod("kube-system", "coredns-1", { workload: { kind: "Deployment", name: "coredns" } }),
      pod("shop", "web-a", {
        project: "shop",
        projectName: "Shop",
        workload: { kind: "Deployment", name: "web" },
      }),
      pod("shop", "web-b", {
        project: "shop",
        workload: { kind: "Deployment", name: "web" },
        scratch: ["tmp"],
        budget: "web",
        budgetBlocks: true,
      }),
      pod("shop", "debug", { project: "shop" }),
    ])
    expect(groups.map((g) => [g.label, g.workloads.map((w) => [w.name, w.count])])).toEqual([
      [
        "Shop",
        [
          ["web", 2],
          ["debug", 1],
        ],
      ],
      ["kube-system", [["coredns", 1]]],
    ])
    expect(groups[0]?.workloads[0]).toMatchObject({ scratch: true, budget: "web" })
  })

  test("names the kubectl command that does the same", () => {
    expect(kubectlCommand("drain", "vps-3", false)).toBe(
      "kubectl drain vps-3 --ignore-daemonsets --delete-emptydir-data",
    )
    expect(kubectlCommand("drain", "vps-3", true)).toEndWith(" --force")
    expect(kubectlCommand("uncordon", "vps-3", false)).toBe("kubectl uncordon vps-3")
  })
})
