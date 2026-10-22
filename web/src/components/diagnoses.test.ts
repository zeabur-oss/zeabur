import { describe, test, expect } from "bun:test"

import type { Schemas } from "@/api/client"
import { logsReadable } from "@/api/logs-access"
import { diagnosisAction, type Diagnosis } from "@/components/diagnoses"
import { en } from "@/i18n/en"
import { unknownRights } from "@/model/rights"
import { reasonWord, statusOf, type Service } from "@/model/service"

const withLogs = (readLogs: Schemas["Permission"]) => ({ can: { ...unknownRights(), readLogs } })

const diagnosis = (kind: Diagnosis["kind"], cause: string): Diagnosis => ({
  kind,
  cause: { key: cause },
  why: { key: `${cause}.why` },
  fix: { key: `${cause}.fix` },
  evidence: [],
  pods: ["web-1"],
})

describe("a diagnosis's action", () => {
  test("follows a concrete fix to where it is done", () => {
    const held = { ...diagnosis("unexplained", "x"), fix: { key: "fix.multiAttach" } }
    expect(diagnosisAction(held).tab).toBe("yaml")
    const oom = { ...diagnosis("unexplained", "x"), fix: { key: "fix.outOfMemory" } }
    expect(diagnosisAction(oom).label).toBe("diagnosis.action.memory")
  })

  test("opens the events instead of logs for someone who may not read logs", () => {
    const crash = diagnosis("crashLoop", "diagnosis.crashLoop.cause.exitCode")
    expect(diagnosisAction(crash, false)).toStrictEqual({
      tab: "events",
      label: "diagnosis.action.events",
      pod: "web-1",
    })
    expect(diagnosisAction(diagnosis("outOfMemory", "x"), false).tab).toBe("compute")
    expect(logsReadable(withLogs("denied"))).toBe(false)
    expect(logsReadable(withLogs("unknown"))).toBe(true)
    expect(logsReadable(undefined)).toBe(true)
  })

  test("opens the tab its fix is done in", () => {
    expect(
      diagnosisAction(diagnosis("crashLoop", "diagnosis.crashLoop.cause.exitCode")),
    ).toStrictEqual({
      tab: "logs",
      label: "diagnosis.action.logs",
      pod: "web-1",
    })
    expect(diagnosisAction(diagnosis("outOfMemory", "x")).tab).toBe("compute")
    expect(diagnosisAction(diagnosis("probeFailure", "x"))).toStrictEqual({
      tab: "health-checks",
      label: "diagnosis.action.healthChecks",
    })
    expect(diagnosisAction(diagnosis("configMissing", "x")).tab).toBe("variables")
    expect(diagnosisAction(diagnosis("quotaBlocked", "x")).label).toBe("diagnosis.action.replicas")
  })

  test("follows the cause where one kind has several", () => {
    const unreachable = (cause: string) =>
      diagnosisAction(diagnosis("unreachable", `diagnosis.unreachable.cause.${cause}`)).tab
    expect(unreachable("noService")).toBe("networking")
    expect(unreachable("targetPort")).toBe("networking")
    expect(unreachable("noReadyEndpoints")).toBe("events")
    const unschedulable = (cause: string) =>
      diagnosisAction(diagnosis("unschedulable", `diagnosis.unschedulable.cause.${cause}`)).label
    expect(unschedulable("cpu")).toBe("diagnosis.action.reservation")
    expect(unschedulable("placement")).toBe("diagnosis.action.events")
    const configMissing = (cause: string) =>
      diagnosisAction(diagnosis("configMissing", `diagnosis.configMissing.cause.${cause}`)).tab
    expect(configMissing("object")).toBe("variables")
    expect(configMissing("volume")).toBe("storage")
    const unexplained = (cause: string) =>
      diagnosisAction(diagnosis("unexplained", `diagnosis.unexplained.cause.${cause}`)).tab
    expect(unexplained("notReady")).toBe("logs")
    expect(unexplained("waiting")).toBe("events")
  })

  test("has words in the catalogue for every action", () => {
    const kinds: Diagnosis["kind"][] = [
      "crashLoop",
      "runFailed",
      "outOfMemory",
      "imagePull",
      "probeFailure",
      "unreachable",
      "quotaBlocked",
      "unschedulable",
      "configMissing",
      "unexplained",
    ]
    for (const kind of kinds)
      expect(en[diagnosisAction(diagnosis(kind, "x")).label], kind).toBeTruthy()
  })
})

describe("the status's plain word", () => {
  test("says what a raw reason means; an unknown reason leaves the status word", () => {
    expect(reasonWord("CrashLoopBackOff", false)).toBe("crashing")
    expect(reasonWord("ImagePullBackOff", false)).toBe("imagePull")
    expect(reasonWord("ErrImagePull", false)).toBe("imagePull")
    expect(reasonWord("OOMKilled", true)).toBe("outOfMemory")
    expect(reasonWord("SomethingNew", false)).toBe(undefined)
    expect(reasonWord(undefined, false)).toBe(undefined)
    for (const word of ["crashing", "imagePull", "outOfMemory", "runFailed"] as const)
      expect(en[`reason.${word}`]).toBeTruthy()
  })

  test("reads Error as a crash for a service and as a failed run for a task", () => {
    expect(reasonWord("Error", false)).toBe("crashing")
    expect(reasonWord("Error", true)).toBe("runFailed")
  })

  test("rests on the failing pod: running again after a recent crash, its restarts and last exit", () => {
    const pod = (extra: Partial<Schemas["PodSummary"]>): Schemas["PodSummary"] => ({
      id: "projects/shop/pods/worker-1",
      name: "worker-1",
      containers: ["worker"],
      health: "fail",
      origin: { kind: "Pod", name: "worker-1" },
      phase: "Running",
      ready: true,
      restarts: 5,
      containerStatuses: [
        {
          name: "worker",
          ready: true,
          restarts: 5,
          lastTermination: { exitCode: 1, reason: "Error", finished: "2026-10-17T02:57:13Z" },
        },
      ],
      ...extra,
    })
    const service = (p: Schemas["PodSummary"]) =>
      ({
        kind: "Deployment",
        name: "worker",
        status: { health: "fail", word: "failing", reason: "Error" },
        replicas: { ready: 1, desired: 1 },
        pods: [p],
      }) as unknown as Service
    const running = statusOf(service(pod({})))
    expect(running.task).toBe(false)
    expect(running.pod).toStrictEqual({
      running: true,
      restarts: 5,
      lastExit: { code: 1, finished: "2026-10-17T02:57:13Z" },
    })
    const waiting = statusOf(service(pod({ ready: false, reason: "CrashLoopBackOff" })))
    expect(waiting.pod?.running).toBe(false)
  })
})
