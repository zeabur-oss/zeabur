import { describe, expect, test } from "bun:test"

import type { Schemas } from "@/api/client"
import { en } from "@/i18n/en"
import { eventWord, explainedBy, overviewOf } from "@/model/overview"
import type { Event } from "@/model/service"

type Diagnosis = Schemas["Diagnosis"]
type Rollout = Schemas["RolloutProgress"]
type Route = Schemas["RouteTraceView"]

const diagnosis = (
  kind: Diagnosis["kind"],
  cause: string,
  more: Partial<Diagnosis> = {},
): Diagnosis => ({
  kind,
  cause: { key: `diagnosis.${kind}.cause.${cause}` },
  why: { key: "w" },
  fix: { key: "f" },
  evidence: [],
  pods: [],
  ...more,
})

const imagePull = diagnosis("imagePull", "notFound", {
  pods: ["web-1"],
  evidence: [{ kind: "Event", name: "b1" }],
})
const noReady = diagnosis("unreachable", "noReadyEndpoints")

const hop = (step: Schemas["HopStep"], status: Schemas["HopStatus"]): Schemas["Hop"] => ({
  step,
  status,
  message: { key: `trace.${step}` },
})

const route = (
  kind: Route["kind"],
  address: string,
  hops: Schemas["Hop"][],
  status: Route["status"] = hops.some((h) => h.status === "failed") ? "failed" : "ok",
): Route => ({ kind, address, hops, status, via: { kind: "Service", name: "web" } })

const blogDomain = route("domain", "blog.example.com", [
  hop("dns", "unknown"),
  hop("service", "ok"),
  hop("ready", "failed"),
])
const privateAddress = route("private", "web.blog.svc.cluster.local", [
  hop("service", "ok"),
  hop("ready", "failed"),
])

const event = (name: string, reason: string, pod: string, type = "Warning"): Event => ({
  id: `projects/blog/events/${name}`,
  origin: { kind: "Event", name },
  regarding: { kind: "Pod", name: pod },
  reason,
  note: "",
  count: 1,
  type,
})

const rollout = (more: Partial<Rollout>): Rollout => ({
  id: "r",
  workload: { kind: "Deployment", name: "web" },
  status: "progressing",
  message: { key: "rollout.progressing" },
  desired: 1,
  updated: 1,
  updatedReady: 0,
  old: 1,
  oldReady: 1,
  revision: 5,
  evidence: [],
  unreadable: [],
  ...more,
})

describe("a service's overview", () => {
  test("says a cause once and folds what follows from it", () => {
    const o = overviewOf(
      { diagnoses: [imagePull, noReady], stopped: false },
      undefined,
      [blogDomain, privateAddress],
      [],
    )
    expect(o.causes).toStrictEqual([imagePull])
    expect(o.waiting.get("blog.example.com")).toBe("cause")
    expect(o.waiting.get("web.blog.svc.cluster.local")).toBe("cause")
    expect(o.alsoDown).toStrictEqual({ domains: ["blog.example.com"], private: true })
  })

  test("keeps no ready replica as the cause when nothing else explains it", () => {
    const o = overviewOf({ diagnoses: [noReady], stopped: false }, undefined, [privateAddress], [])
    expect(o.causes).toStrictEqual([noReady])
    expect(o.alsoDown).toStrictEqual({ domains: [], private: true })
  })

  test("leaves an address red when its own hop fails", () => {
    const wrongPort = route("domain", "shop.example.com", [
      hop("targetPort", "failed"),
      hop("ready", "failed"),
    ])
    const o = overviewOf({ diagnoses: [imagePull], stopped: false }, undefined, [wrongPort], [])
    expect(o.waiting.size).toBe(0)
    expect(o.alsoDown).toBeUndefined()
  })

  test("says an address of a service scaled to zero waits for it, without a cause", () => {
    const o = overviewOf({ diagnoses: [], stopped: true }, undefined, [privateAddress], [])
    expect(o.waiting.get("web.blog.svc.cluster.local")).toBe("stopped")
    expect(o.alsoDown).toBeUndefined()
  })

  test("without a diagnosis or a known reason, an address stays not reachable", () => {
    const o = overviewOf({ diagnoses: [], stopped: false }, undefined, [privateAddress], [])
    expect(o.waiting.size).toBe(0)
  })
})

describe("a rollout on the overview", () => {
  const shown = (r: Rollout | undefined, diagnoses: Diagnosis[] = []) =>
    overviewOf({ diagnoses, stopped: false }, r, [], []).rollout

  test("gets a card only while it moves by itself", () => {
    expect(shown(rollout({}))).toStrictEqual({ as: "card" })
    expect(shown(rollout({ status: "done" }))).toStrictEqual({ as: "none" })
    expect(shown(rollout({ desired: 0 }))).toStrictEqual({ as: "none" })
    expect(shown(undefined)).toStrictEqual({ as: "none" })
  })

  test("joins the cause it waits on, and is stuck when that cause never passes", () => {
    expect(shown(rollout({}), [imagePull])).toStrictEqual({ as: "line", stuck: true })
    expect(shown(rollout({ status: "stuck" }), [imagePull])).toStrictEqual({
      as: "line",
      stuck: true,
    })
    const probe = diagnosis("probeFailure", "readiness")
    expect(shown(rollout({}), [probe])).toStrictEqual({ as: "line", stuck: false })
  })

  test("says nothing of its own when no release is under way", () => {
    const settled = rollout({ old: 0, updated: 1 })
    expect(shown(settled, [imagePull])).toStrictEqual({ as: "none" })
    expect(shown(rollout({ revision: null }), [imagePull])).toStrictEqual({ as: "none" })
  })
})

describe("warnings on the overview", () => {
  test("leave out what a diagnosis explains", () => {
    const backOff = event("b2", "BackOff", "web-1")
    const failed = event("b1", "Failed", "web-1")
    const other = event("x", "BackOff", "web-2")
    const normal = event("n", "Pulled", "web-1", "Normal")
    const o = overviewOf(
      { diagnoses: [imagePull], stopped: false },
      undefined,
      [],
      [backOff, failed, other, normal],
    )
    expect(o.warnings).toStrictEqual([other])
  })

  test("leave out what a failed run explains on its Job", () => {
    const runFailed = diagnosis("runFailed", "exitCode", {
      pods: ["run-1-abc"],
      evidence: [{ kind: "Job", name: "run-1", fieldPath: "status.conditions[type=Failed]" }],
    })
    const gaveUp: Event = {
      ...event("i1", "BackoffLimitExceeded", "x"),
      regarding: { kind: "Job", name: "run-1" },
    }
    expect(explainedBy(gaveUp, [runFailed])).toBe(true)
    expect(explainedBy({ ...gaveUp, reason: "SuccessfulCreate" }, [runFailed])).toBe(false)
  })

  test("have plain words for the usual reasons, and none for an unknown one", () => {
    expect(eventWord("BackOff")).toBe("retrying")
    expect(eventWord("SomethingNew")).toBeUndefined()
    expect(eventWord("toString")).toBeUndefined()
    for (const reason of [
      "BackOff",
      "Unhealthy",
      "Killing",
      "Failed",
      "FailedScheduling",
      "FailedMount",
      "FailedCreate",
      "BackoffLimitExceeded",
      "Evicted",
      "OOMKilling",
    ]) {
      const word = eventWord(reason)
      expect(word, reason).toBeDefined()
      if (word) expect(en[`event.word.${word}`]).toBeTruthy()
    }
  })
})
