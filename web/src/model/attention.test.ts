import { describe, expect, test } from "bun:test"

import { groupProblems, sharedCause } from "@/model/attention"
import type { Service } from "@/model/service"

const pull = (container: string, image: string) => ({
  kind: "imagePull",
  container,
  pods: [],
  cause: { key: "diagnosis.imagePull.cause.notFound", params: { container, image } },
  why: { key: "diagnosis.imagePull.why" },
  fix: { key: "diagnosis.imagePull.fix.notFound" },
  evidence: [],
})

const service = (name: string, health: string, diagnoses: unknown[] = []) =>
  ({
    id: `projects/shop/services/deployment/${name}`,
    kind: "Deployment",
    name,
    pods: [],
    containers: [],
    endpointNames: [],
    routeNames: [],
    status: { health, word: health === "ok" ? "ready" : "failed" },
    diagnoses,
  }) as unknown as Service

const names = (services: Service[]) =>
  groupProblems(services).map((g) => g.services.map((s) => s.name))

describe("Needs attention", () => {
  test("is empty when nothing needs attention", () => {
    expect(groupProblems([])).toStrictEqual([])
    expect(groupProblems([service("api", "ok")])).toStrictEqual([])
  })

  test("holds one service in trouble as a group of one", () => {
    expect(
      names([service("api", "fail", [pull("api", "a:1")]), service("web", "ok")]),
    ).toStrictEqual([["api"]])
  })

  test("says a cause once for every service it stops, whatever their names", () => {
    const services = [
      service("b", "fail", [pull("b", "a:1")]),
      service("a", "fail", [pull("a", "a:1")]),
      service("c", "fail", [pull("c", "other:2")]),
    ]
    expect(names(services)).toStrictEqual([["a", "b"], ["c"]])
  })

  test("puts failing groups before degraded ones, then the largest first", () => {
    const services = [
      service("w1", "warn"),
      service("w2", "warn"),
      service("w3", "warn"),
      service("f", "fail", [pull("f", "x:1")]),
      ...Array.from({ length: 40 }, (_, i) => service(`s${i}`, "fail", [pull(`s${i}`, "y:1")])),
    ]
    const groups = groupProblems(services)
    expect(groups.map((g) => g.services.length)).toStrictEqual([40, 1, 3])
  })
})

describe("a shared cause", () => {
  test("names no one service", () => {
    expect(sharedCause(pull("api", "a:1").cause, "Each one")).toStrictEqual({
      key: "diagnosis.imagePull.cause.notFound",
      params: { container: "Each one", image: "a:1" },
    })
  })

  test("is left out when the sentence names the service another way", () => {
    const cause = {
      key: "diagnosis.unreachable.cause.noReadyEndpoints",
      params: { endpoint: "web", service: "web" },
    }
    expect(sharedCause(cause, "Each one")).toBeUndefined()
  })
})
