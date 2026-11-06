import { describe, expect, test } from "bun:test"

import type { Schemas } from "@/api/client"
import type { Build } from "@/model/builds"
import type { Service } from "@/model/service"
import { wrongPort } from "@/model/wrong-port"

const IMAGE = "zot.zot.svc:5000/lab/checkout:main-1-5d41402"
const NOW = Date.parse("2026-10-26T09:00:00Z")
const ago = (seconds: number) => new Date(NOW - seconds * 1000).toISOString()

const build = (exposedPorts?: string[]) =>
  ({
    name: "checkout",
    repository: "zot.zot.svc:5000/lab/checkout",
    latestImage: IMAGE,
    exposedPorts,
  }) as Build

const refused = (output = "dial tcp 10.42.2.31:8080: connect: connection refused") =>
  ({
    kind: "probeFailure",
    container: "checkout",
    pods: ["checkout-1"],
    cause: {
      key: "diagnosis.probeFailure.cause.readiness",
      params: { container: "checkout", count: "28", output },
    },
    why: { key: "diagnosis.probeFailure.why.readiness" },
    fix: { key: "diagnosis.probeFailure.fix.readiness" },
    evidence: [],
  }) as unknown as Schemas.Diagnosis

const checkout = ({ started = 300, diagnoses = [refused()] } = {}) =>
  ({
    kind: "Deployment",
    name: "checkout",
    namespace: "lab",
    containers: [
      {
        name: "checkout",
        image: IMAGE,
        ports: [{ port: 8080 }],
        probes: { readiness: { port: "8080" } },
      },
    ],
    pods: [
      {
        name: "checkout-1",
        containerStatuses: [{ name: "checkout", ready: false, startedAt: ago(started) }],
      },
    ],
    diagnoses,
  }) as unknown as Service

describe("a built service that doesn't listen on its port", () => {
  test("is said once its traffic check has been refused for more than a minute", () => {
    const wrong = wrongPort(checkout(), [build(["3000/tcp"])], NOW)
    expect(wrong?.port).toBe(8080)
    expect(wrong?.diagnosis.kind).toBe("probeFailure")
  })

  test("is not said while a slow app still has its first minute to start listening", () => {
    expect(wrongPort(checkout({ started: 40 }), [build(["3000/tcp"])], NOW)).toBeUndefined()
  })

  test("is not said when the check fails for another reason or on another port", () => {
    const timeout = refused("dial tcp 10.42.2.31:8080: i/o timeout")
    expect(wrongPort(checkout({ diagnoses: [timeout] }), [build()], NOW)).toBeUndefined()
    const other = refused("dial tcp 10.42.2.31:9090: connect: connection refused")
    expect(wrongPort(checkout({ diagnoses: [other] }), [build()], NOW)).toBeUndefined()
  })

  test("is not said of a service no Build makes", () => {
    expect(wrongPort(checkout(), [], NOW)).toBeUndefined()
  })

  test("offers the port the image declares when it differs", () => {
    expect(wrongPort(checkout(), [build(["3000/tcp", "9000/tcp"])], NOW)?.offer).toStrictEqual({
      container: "checkout",
      from: 8080,
      to: 3000,
    })
  })

  test("offers nothing when the image declares the service's port or only UDP ports", () => {
    expect(wrongPort(checkout(), [build(["8080/tcp", "3000/tcp"])], NOW)?.offer).toBeUndefined()
    expect(wrongPort(checkout(), [build(["53/udp"])], NOW)?.offer).toBeUndefined()
  })

  test("is still said, without an offer, when the image declares no port", () => {
    const wrong = wrongPort(checkout(), [build()], NOW)
    expect(wrong?.port).toBe(8080)
    expect(wrong?.offer).toBeUndefined()
  })
})
