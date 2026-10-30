import { describe, expect, test } from "bun:test"

import type { Schemas } from "@/api/client"
import { nodeIssues } from "@/model/node-health"
import type { Overview } from "@/model/overview"
import type { Status } from "@/model/service"
import {
  activityTone,
  entryTone,
  logTone,
  reachHealth,
  replicasHealth,
  SCHEDULING_GRACE_MS,
  schedulingHealth,
  severity,
  timelineVariant,
  troubleTone,
  warningTone,
  warningVariant,
  worstOf,
} from "@/model/severity"
import { verdictOf } from "@/model/verdict"

type Diagnosis = Schemas.Diagnosis
type Route = Schemas.RouteTraceView

const now = Date.parse("2026-10-23T12:00:00Z")
const ago = (ms: number) => new Date(now - ms).toISOString()

const diagnosis = (kind: Diagnosis["kind"], pods: string[] = []): Diagnosis => ({
  kind,
  cause: { key: `diagnosis.${kind}.cause.x` },
  why: { key: "w" },
  fix: { key: "f" },
  evidence: [],
  pods,
})

const route = (address: string, status: Route["status"]): Route => ({
  kind: "domain",
  address,
  status,
  hops: [],
  via: { kind: "Ingress", name: "web" },
})

const toneOf = (
  status: Status,
  { causes = [], routes = [], pods = [], rollout = { as: "none" } as Overview["rollout"] } = {} as {
    causes?: Diagnosis[]
    routes?: Route[]
    pods?: { name: string; created?: string }[]
    rollout?: Overview["rollout"]
  },
) =>
  verdictOf({
    status,
    said: { causes, rollout, waiting: new Map() },
    routes,
    checks: () => undefined,
    pods,
    now,
  }).tone

const node = (more: Partial<Schemas.NodeView> = {}): Schemas.NodeView => ({
  id: "nodes/vps-1",
  name: "vps-1",
  allocatable: { cpuMillicores: 2000, memoryBytes: 2 ** 31, pods: 110 },
  capacity: { cpuMillicores: 2000, memoryBytes: 2 ** 31, pods: 110 },
  architecture: "amd64",
  containerRuntime: "containerd://1.7",
  cordoned: false,
  kernelVersion: "6.1",
  kubeletVersion: "v1.31.4+k3s1",
  osImage: "Debian",
  origin: { kind: "Node", name: "vps-1" },
  problems: [],
  ready: "True",
  roles: [],
  taints: [],
  ...more,
})

describe("red: not doing its job and won't recover without someone acting", () => {
  test("a failed run", () => {
    expect(toneOf({ health: "fail", word: "failing", reason: "Error", task: true })).toBe("fail")
    expect(severity.runFailed).toBe("fail")
  })

  test("keeps crashing, even while the release before still serves", () => {
    const status: Status = { health: "fail", word: "rolloutStuck", reason: "CrashLoopBackOff" }
    expect(toneOf(status, { causes: [diagnosis("crashLoop")] })).toBe("fail")
    expect(troubleTone(status.health)).toBe("fail")
  })

  test("can't pull its image", () => {
    const status: Status = { health: "fail", word: "failing", reason: "ImagePullBackOff" }
    expect(toneOf(status, { causes: [diagnosis("imagePull")] })).toBe("fail")
  })

  test("can't be scheduled once its first minutes are over", () => {
    const status: Status = { health: "progress", word: "rollingOut", ready: 0, desired: 1 }
    const pods = [{ name: "web-1", created: ago(SCHEDULING_GRACE_MS + 1000) }]
    expect(toneOf(status, { causes: [diagnosis("unschedulable", ["web-1"])], pods })).toBe("fail")
    expect(schedulingHealth(ago(SCHEDULING_GRACE_MS + 1000), now)).toBe("fail")
  })

  test("no ready replica", () => {
    expect(replicasHealth(0, 2)).toBe("fail")
  })

  test("a domain nobody reaches", () => {
    const ready: Status = { health: "ok", word: "ready", ready: 2, desired: 2 }
    expect(toneOf(ready, { routes: [route("shop.example.com", "failed")] })).toBe("fail")
    expect(reachHealth.unreachable).toBe("fail")
  })

  test("a certificate that failed", () => {
    expect(severity.certificateFailed).toBe("fail")
  })

  test("a node that is not ready", () => {
    expect(nodeIssues(node({ ready: "False" }))[0]?.health).toBe("fail")
  })

  test("an error log line", () => {
    expect(logTone.error).toBe("fail")
  })
})

describe("amber: still serving, but degraded or needs attention soon", () => {
  test("a Kubernetes Warning event", () => {
    expect(entryTone.warning).toBe("warn")
    expect(severity.warningEvent).toBe("warn")
  })

  test("a restart while it runs now", () => {
    expect(entryTone.termination).toBe("warn")
  })

  test("fewer ready replicas than wanted, one at least", () => {
    expect(replicasHealth(1, 2)).toBe("warn")
    expect(toneOf({ health: "warn", word: "degraded", ready: 1, desired: 2 })).toBe("warn")
  })

  test("errors in the app while it serves", () => {
    const serving: Status = { health: "warn", word: "rolloutStuck", ready: 1, desired: 1 }
    expect(toneOf(serving, { causes: [diagnosis("unexplained")] })).toBe("warn")
    expect(troubleTone(serving.health)).toBe("warn")
  })

  test("DNS that points elsewhere", () => {
    expect(reachHealth.elsewhere).toBe("warn")
  })

  test("more reserved than a node holds", () => {
    const reserved = nodeIssues(
      node({ requests: { cpuMillicores: 1900, memoryBytes: 0, pods: 3 } }),
    ).find((i) => i.kind === "reservedCpu")
    expect(reserved?.health).toBe("warn")
  })

  test("a warning log line", () => {
    expect(logTone.warn).toBe("warn")
  })
})

describe("neutral: on its way", () => {
  test("deploying and rolling out", () => {
    expect(toneOf({ health: "progress", word: "rollingOut", ready: 1, desired: 2 })).toBe(
      "progress",
    )
  })

  test("issuing a certificate", () => {
    expect(reachHealth.issuing).toBe("progress")
  })

  test("scheduling in its first minutes", () => {
    const status: Status = { health: "progress", word: "rollingOut", ready: 0, desired: 1 }
    const pods = [{ name: "web-1", created: ago(60_000) }]
    expect(toneOf(status, { causes: [diagnosis("unschedulable", ["web-1"])], pods })).toBe(
      "progress",
    )
  })
})

describe("one mark per row", () => {
  test("a group takes its worst member's tone", () => {
    expect(worstOf(["ok", "warn", "idle"])).toBe("warn")
    expect(worstOf(["ok", "fail", "warn"])).toBe("fail")
    expect(worstOf(["ok", "idle"])).toBe("ok")
  })

  test("a problem's pill takes its service's tone", () => {
    expect(troubleTone("fail")).toBe("fail")
    expect(troubleTone("warn")).toBe("warn")
  })
})

describe("Warning events by what they say", () => {
  test("a crash loop is red, however it reached Activity", () => {
    expect(warningTone(timelineVariant("timeline.warning.crashLoop"))).toBe("fail")
    expect(
      warningTone(
        warningVariant({ reason: "BackOff", note: "Back-off restarting failed container" }),
      ),
    ).toBe("fail")
    expect(warningTone(warningVariant({ reason: "CrashLoopBackOff" }))).toBe("fail")
  })

  test("an image that cannot be pulled is red", () => {
    expect(warningVariant({ reason: "BackOff", note: 'Back-off pulling image "x"' })).toBe(
      "imagePull",
    )
    expect(warningTone("imagePull")).toBe("fail")
  })

  test("waiting for a node is red only past the allowance, which the service's health says", () => {
    const missing = timelineVariant("timeline.warning.unschedulable.claimMissing")
    expect(warningTone(missing, "fail")).toBe("fail")
    expect(warningTone(missing, "progress")).toBe("warn")
    expect(warningTone(warningVariant({ reason: "FailedScheduling" }), "fail")).toBe("fail")
  })

  test("other warnings stay amber", () => {
    expect(warningTone(timelineVariant("timeline.warning.probe"), "fail")).toBe("warn")
    expect(warningTone(warningVariant({ reason: "Unhealthy" }))).toBe("warn")
  })
})

describe("Activity entries", () => {
  test("a container that ended is red while its service is failing, amber once it runs", () => {
    const ended = { kind: "termination" as const, message: { key: "timeline.termination.exited" } }
    expect(activityTone(ended, "fail")).toBe("fail")
    expect(activityTone(ended, "ok")).toBe("warn")
  })

  test("a crash loop warning is red", () => {
    expect(
      activityTone({ kind: "warning", message: { key: "timeline.warning.crashLoop" } }, "ok"),
    ).toBe("fail")
  })
})
