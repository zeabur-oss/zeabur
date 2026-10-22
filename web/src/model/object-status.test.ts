import { describe, expect, test } from "bun:test"

import { conditionHealth, statusParts, toYaml } from "@/model/object-status"

describe("statusParts", () => {
  test("a Deployment: conditions apart, counts as pairs", () => {
    const parts = statusParts({
      availableReplicas: 2,
      conditions: [{ type: "Available", status: "True", reason: "MinimumReplicasAvailable" }],
      observedGeneration: 4,
      replicas: 2,
    })
    expect(parts.conditions.map((c) => c.type)).toStrictEqual(["Available"])
    expect(parts.fields).toStrictEqual([
      ["availableReplicas", "2"],
      ["observedGeneration", "4"],
      ["replicas", "2"],
    ])
    expect(parts.nested).toBeUndefined()
  })

  test("a claim: phase first, short lists and maps on one line", () => {
    const parts = statusParts({
      accessModes: ["ReadWriteOnce"],
      capacity: { storage: "1Gi" },
      phase: "Bound",
    })
    expect(parts.fields).toStrictEqual([
      ["phase", "Bound"],
      ["accessModes", "ReadWriteOnce"],
      ["capacity.storage", "1Gi"],
    ])
  })

  test("what does not fit on a line is kept for YAML; empty values are left out", () => {
    const parts = statusParts({
      loadBalancer: {},
      containerStatuses: [{ name: "web", ready: true }],
      conditions: [{ lastProbeTime: null }],
    })
    expect(parts.fields).toStrictEqual([])
    expect(parts.conditions).toStrictEqual([])
    expect(Object.keys(parts.nested ?? {})).toStrictEqual(["containerStatuses", "conditions"])
  })

  test("no status", () => {
    expect(statusParts(undefined)).toStrictEqual({ conditions: [], fields: [] })
  })
})

describe("conditionHealth", () => {
  test("by what the condition means", () => {
    expect(conditionHealth({ type: "Available", status: "True" })).toBe("ok")
    expect(conditionHealth({ type: "Available", status: "False" })).toBe("warn")
    expect(conditionHealth({ type: "ReplicaFailure", status: "True" })).toBe("warn")
    expect(conditionHealth({ type: "DiskPressure", status: "False" })).toBe("ok")
    expect(conditionHealth({ type: "Ready", status: "Unknown" })).toBe("idle")
  })
})

describe("toYaml", () => {
  test("nests maps and lists as kubectl does and quotes what would read as something else", () => {
    expect(
      toYaml({
        containerStatuses: [
          { name: "web", ready: true, state: { running: { startedAt: "2026-10-23T01:02:03Z" } } },
        ],
        ingress: [{ ip: "10.0.0.1", ports: [] }],
        note: "yes",
        message: "back-off 10s: restarting",
      }),
    ).toBe(
      [
        "containerStatuses:",
        "- name: web",
        "  ready: true",
        "  state:",
        "    running:",
        '      startedAt: "2026-10-23T01:02:03Z"',
        "ingress:",
        '- ip: "10.0.0.1"',
        "  ports: []",
        'note: "yes"',
        'message: "back-off 10s: restarting"',
        "",
      ].join("\n"),
    )
  })
})
