import { describe, expect, test } from "bun:test"

import {
  activityTone,
  entryTone,
  logTone,
  reachHealth,
  severity,
  timelineVariant,
  troubleTone,
  warningTone,
  worstOf,
} from "@/model/severity"

describe("red: not doing its job and won't recover without someone acting", () => {
  test("a failed run", () => {
    expect(severity.runFailed).toBe("fail")
  })

  test("a domain nobody reaches", () => {
    expect(reachHealth.unreachable).toBe("fail")
  })

  test("a certificate that failed", () => {
    expect(severity.certificateFailed).toBe("fail")
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

  test("DNS that points elsewhere", () => {
    expect(reachHealth.elsewhere).toBe("warn")
  })

  test("a warning log line", () => {
    expect(logTone.warn).toBe("warn")
  })
})

describe("neutral: on its way", () => {
  test("issuing a certificate", () => {
    expect(reachHealth.issuing).toBe("progress")
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
  test("a crash loop or an image that cannot be pulled is red", () => {
    expect(warningTone(timelineVariant("timeline.warning.crashLoop"))).toBe("fail")
    expect(warningTone("crashLoop")).toBe("fail")
    expect(warningTone("imagePull")).toBe("fail")
  })

  test("waiting for a node is red only past the allowance, which the service's health says", () => {
    const missing = timelineVariant("timeline.warning.unschedulable.claimMissing")
    expect(warningTone(missing, "fail")).toBe("fail")
    expect(warningTone(missing, "progress")).toBe("warn")
    expect(warningTone("unschedulable", "fail")).toBe("fail")
  })

  test("other warnings stay amber", () => {
    expect(warningTone(timelineVariant("timeline.warning.probe"), "fail")).toBe("warn")
    expect(warningTone("other")).toBe("warn")
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
