import { describe, test, expect } from "bun:test"

import type { Schemas } from "@/api/client"
import { en } from "@/i18n/en"
import { zhCN } from "@/i18n/zh-CN"
import { zhTW } from "@/i18n/zh-TW"
import {
  aroundName,
  awaitsName,
  destinationOf,
  isolates,
  typedName,
  undoOf,
  weigh,
  type Pods,
  type PodsOf,
  type Risk,
} from "@/model/risk"
import { forAll } from "@/test/property"

type Change = Schemas["Change"]

const api = { kind: "Deployment", name: "api" } as const
const pods =
  (known: Record<string, Pods>): PodsOf =>
  (workload) =>
    known[workload.name]

const policies = { group: "networking.k8s.io", version: "v1", resource: "networkpolicies" }
const remove: Change = { op: "delete", resource: policies, name: "zeabur-isolate-api" }

const isolate = {
  apiVersion: "networking.k8s.io/v1",
  kind: "NetworkPolicy",
  metadata: { name: "zeabur-isolate-api" },
  spec: {
    podSelector: { matchLabels: { app: "api" } },
    policyTypes: ["Ingress", "Egress"],
    ingress: [{ from: [{ podSelector: {} }] }],
    egress: [{ to: [{ podSelector: {} }] }],
  },
} as unknown as Record<string, never>

describe("weigh", () => {
  test("a restart with two or more ready pods is low, with fewer high", () => {
    const restart: Change[] = [{ op: "restart", workload: api }]
    expect(weigh(restart, pods({ api: { desired: 3, ready: 3 } }))).toStrictEqual({
      risk: "low",
      reasons: [],
    })
    expect(weigh(restart, pods({ api: { desired: 3, ready: 1 } }))).toStrictEqual({
      risk: "high",
      reasons: ["fewPods"],
    })
    expect(weigh(restart, pods({})).risk).toBe("high")
  })

  test("scaling is low, except to zero", () => {
    expect(weigh([{ op: "scale", workload: api, replicas: 4 }], pods({})).risk).toBe("low")
    expect(weigh([{ op: "scale", workload: api, replicas: 0 }], pods({})).reasons).toStrictEqual([
      "scaleToZero",
    ])
  })

  test("deleting a key is high, setting one normal", () => {
    const data = (op: string): Change[] => [
      { op: "data", kind: "Secret", name: "db", changes: [{ op, key: "password", value: "x" }] },
    ]
    expect(weigh(data("set"), pods({})).risk).toBe("normal")
    expect(weigh(data("remove"), pods({}))).toStrictEqual({
      risk: "high",
      reasons: ["deletesKeys"],
    })
  })

  test("rollback, storage, YAML edits and removing a domain are high", () => {
    const high: [Change, string][] = [
      [{ op: "rollback", workload: api, revision: 5 }, "rollback"],
      [{ op: "expandVolume", claim: "data", size: "20Gi" }, "storage"],
      [{ op: "addVolume", workload: api, volume: { mountPath: "/data", size: "5Gi" } }, "storage"],
      [{ op: "editMount", workload: api, mountPath: "/data", readOnly: true }, "storage"],
      [
        {
          op: "edit",
          resource: { group: "", version: "v1", resource: "configmaps" },
          name: "c",
          yaml: "",
        },
        "yamlEdit",
      ],
      [
        { op: "removeDomain", route: { kind: "Ingress", name: "api" }, hostname: "a.example" },
        "removesDomain",
      ],
    ]
    for (const [change, reason] of high)
      expect<unknown>(weigh([change], pods({})), change.op).toStrictEqual({
        risk: "high",
        reasons: [reason],
      })
  })

  test("deleting an object is high", () => {
    expect(weigh([remove], pods({}))).toStrictEqual({ risk: "high", reasons: ["deletes"] })
  })

  test("variables, resources, probes and new domains are normal", () => {
    const normal: Change[] = [
      { op: "env", workload: api, changes: [{ op: "remove", name: "A" }] },
      { op: "resources", workload: api, requests: { cpu: "100m" } },
      { op: "probe", workload: api, probe: "readiness" },
      { op: "addDomain", workload: api, hostname: "a.example" },
      { op: "renameDomain", route: { kind: "Ingress", name: "api" }, from: "a", to: "b" },
    ]
    for (const change of normal) expect(weigh([change], pods({})).risk, change.op).toBe("normal")
  })

  test("a plan takes its highest change, and every reason once, in order", () => {
    // One change of each risk, and every reason, two of them twice.
    const pool: Change[] = [
      { op: "restart", workload: api },
      { op: "scale", workload: api, replicas: 2 },
      { op: "scale", workload: api, replicas: 0 },
      { op: "env", workload: api, changes: [{ op: "set", name: "A", value: "1" }] },
      { op: "data", kind: "Secret", name: "db", changes: [{ op: "remove", key: "password" }] },
      { op: "resources", workload: api, limits: { memory: "1Gi" } },
      { op: "rollback", workload: api, revision: 5 },
      { op: "expandVolume", claim: "data", size: "20Gi" },
      { op: "editMount", workload: api, mountPath: "/data", readOnly: true },
      { op: "removeDomain", route: { kind: "Ingress", name: "api" }, hostname: "a.example" },
      { op: "renameDomain", route: { kind: "Ingress", name: "api" }, from: "a", to: "b" },
      { op: "create", object: isolate },
      { op: "create", object: isolate },
      remove,
    ]
    const rank = { low: 0, normal: 1, high: 2 } as const
    forAll((random) => {
      const known = pods({ api: { desired: 3, ready: random.int(0, 3) } })
      const changes = random.list(6, () => random.pick(pool))
      const each = changes.map((change) => weigh([change], known))
      const highest = each.reduce<Risk>(
        (risk, one) => (rank[one.risk] > rank[risk] ? one.risk : risk),
        "low",
      )
      expect(weigh(changes, known)).toStrictEqual({
        risk: changes.length === 0 ? "normal" : highest,
        reasons: [...new Set(each.flatMap((one) => one.reasons))],
      })
    })
    // Nothing to weigh is not low: a plan without changes is confirmed as usual.
    expect(weigh([], pods({}))).toStrictEqual({ risk: "normal", reasons: [] })
  })

  test("a NetworkPolicy that limits egress or lets nothing in isolates; an allow rule does not", () => {
    expect(isolates(isolate)).toBe(true)
    expect(weigh([{ op: "create", object: isolate }], pods({})).reasons).toStrictEqual(["isolates"])
    const allow = { ...isolate, spec: { policyTypes: ["Ingress"], ingress: [{ from: [] }] } }
    expect(isolates(allow)).toBe(false)
    expect(isolates({ kind: "NetworkPolicy", spec: { policyTypes: ["Ingress"] } })).toBe(true)
    expect(isolates({ kind: "NetworkPolicy", spec: { egress: [] } })).toBe(true)
    expect(isolates({ kind: "ConfigMap" })).toBe(false)
  })
})

describe("IP limits", () => {
  test("limiting where pods connect isolates them; lifting it and limiting domains do not", () => {
    const limit: Change = { op: "egressLimit", workload: api, enabled: true, cidrs: [] }
    expect(weigh([limit], pods({})).reasons).toStrictEqual(["isolates"])
    expect(weigh([{ ...limit, enabled: false }], pods({})).risk).toBe("normal")
    const route: Schemas["RouteRef"] = { kind: "Ingress", name: "web" }
    expect(
      weigh([{ op: "ipAllow", route, enabled: true, cidrs: ["10.0.0.0/8"] }], pods({})),
    ).toStrictEqual({ risk: "normal", reasons: [] })
  })
})

describe("undoOf", () => {
  test("scaling is undone by scaling back to the pods there were", () => {
    const known = pods({ api: { desired: 2, ready: 2 } })
    expect(undoOf([{ op: "scale", workload: api, replicas: 4 }], known)).toStrictEqual([
      { op: "scale", workload: api, replicas: 2 },
    ])
  })

  test("a creation is undone by deleting what was created, at the version it was created at", () => {
    const applied = [
      {
        target: {
          apiVersion: "networking.k8s.io/v1",
          kind: "NetworkPolicy",
          plural: "networkpolicies",
          name: "zeabur-isolate-api",
        },
        resourceVersion: "812",
      },
    ]
    expect(undoOf([{ op: "create", object: isolate }], pods({}), applied)).toStrictEqual([
      { ...remove, resourceVersion: "812" },
    ])
    // Without the version the creation returned there is nothing safe to delete.
    expect(undoOf([{ op: "create", object: isolate }], pods({}))).toBe(undefined)
    expect(
      undoOf([{ op: "create", object: isolate }], pods({}), [
        { ...applied[0], resourceVersion: undefined },
      ]),
    ).toBe(undefined)
  })

  test("nothing else is, nor a scale without a known count", () => {
    expect(undoOf([remove], pods({}))).toBe(undefined)
    expect(undoOf([{ op: "restart", workload: api }], pods({}))).toBe(undefined)
    expect(undoOf([{ op: "scale", workload: api, replicas: 4 }], pods({}))).toBe(undefined)
    expect(
      undoOf(
        [
          { op: "scale", workload: api, replicas: 4 },
          { op: "resources", workload: api },
        ],
        pods({ api: { desired: 2, ready: 2 } }),
      ),
    ).toBe(undefined)
  })
})

describe("typed confirmation", () => {
  test("a deletion waits for the object's name; other plans do not", () => {
    expect(typedName([remove])).toBe("zeabur-isolate-api")
    expect(awaitsName([remove], "")).toBe(true)
    expect(awaitsName([remove], "zeabur-isolate")).toBe(true)
    expect(awaitsName([remove], "ZEABUR-ISOLATE-API")).toBe(true)
    expect(awaitsName([remove], " zeabur-isolate-api ")).toBe(false)
    expect(awaitsName([{ op: "restart", workload: api }], "")).toBe(false)
    expect(typedName([{ op: "restart", workload: api }])).toBe(undefined)
  })

  test("each language's prompt has one place for the name, whatever the name is", () => {
    for (const messages of [en, zhTW, zhCN]) {
      for (const name of ["web", "to", "e", "輸入"]) {
        const [before, after] = aroundName(messages["plan.typeName"])
        expect(`${before}${name}${after}`).toBe(messages["plan.typeName"](name))
      }
    }
  })

  test("deleting a project waits for the project's name, and leads to the projects list", () => {
    const deletion = [{ op: "deleteProject" as const }]
    expect(weigh(deletion, pods({})).risk).toBe("high")
    expect(awaitsName(deletion, "", "shop")).toBe(true)
    expect(awaitsName(deletion, "shop", "shop")).toBe(false)
    expect(destinationOf("shop", deletion, undefined)).toBe("/projects")
  })
})

describe("destinationOf", () => {
  const service = "/projects/shop/services/Deployment/api"
  test("a workload's change goes to its tab", () => {
    expect(destinationOf("shop", [{ op: "restart", workload: api }], undefined)).toBe(
      `${service}?tab=deployments`,
    )
    expect(destinationOf("shop", [{ op: "env", workload: api, changes: [] }], undefined)).toBe(
      `${service}?tab=variables`,
    )
    expect(destinationOf("shop", [{ op: "scale", workload: api, replicas: 2 }], undefined)).toBe(
      `${service}?tab=settings#compute`,
    )
  })

  test("an object's change goes to the object", () => {
    const plan = {
      objects: [
        {
          target: {
            apiVersion: "v1",
            kind: "ConfigMap",
            plural: "configmaps",
            name: "shop-config",
          },
        },
      ],
    } as unknown as Pick<Schemas["PlanResponse"], "objects">
    expect(
      destinationOf(
        "shop",
        [{ op: "data", kind: "ConfigMap", name: "shop-config", changes: [] }],
        plan,
      ),
    ).toBe("/projects/shop/objects?object=core%2Fv1%2Fconfigmaps%2Fshop-config")
  })

  test("a policy goes to the networking tab of the service it was asked from", () => {
    expect(destinationOf("shop", [{ op: "create", object: isolate }], undefined, api)).toBe(
      `${service}?tab=networking`,
    )
  })
})
