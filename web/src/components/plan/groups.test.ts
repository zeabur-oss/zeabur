import { describe as group, expect, test } from "bun:test"

import type { Schemas } from "@/api/client"
import { describe, undoTitle } from "@/components/plan/describe"
import { bounded, editGroups, outcomeGroups, segments } from "@/components/plan/groups"
import { translator, type Translate } from "@/i18n"
import { en } from "@/i18n/en"
import type { Pods, PodsOf } from "@/model/risk"

const show = (value: unknown): string =>
  Array.isArray(value) ? `[${value.map(show).join(", ")}]` : String(value)
const t = ((key: string, ...args: unknown[]) =>
  args.length > 0 ? `${key}(${args.map(show).join(", ")})` : key) as unknown as Translate
const english = translator(en)

type Plan = Pick<Schemas["PlanResponse"], "objects" | "impact" | "warnings"> & {
  preview?: Schemas["Preview"]
}
const api = { kind: "Deployment", name: "api" } as const

const object = (
  target: { kind: string; name: string },
  changes: Schemas["FieldChange"][],
  over: Partial<Schemas["PlannedObject"]> = {},
) =>
  ({
    target: { apiVersion: "apps/v1", plural: "deployments", ...target },
    changes,
    owners: [],
    beforeYaml: "kind: x",
    write: { type: "patch", operations: [] },
    ...over,
  }) as unknown as Schemas["PlannedObject"]

const env = (name: string, leaf: string) =>
  `spec.template.spec.containers[name=api].env[name=${name}].${leaf}`

/** What the demo's planner answers for api: three variables, five replicas, new resources. */
const apiChanges: Schemas["FieldChange"][] = [
  { path: "spec.replicas", before: "3", after: "5" },
  { path: env("FOO", "name"), after: "FOO" },
  { path: env("FOO", "value"), after: "bar" },
  { path: env("LOG_LEVEL", "value"), before: "info", after: "debug" },
  { path: env("REDIS_URL", "name"), before: "REDIS_URL" },
  { path: env("REDIS_URL", "value"), before: "redis://redis:6379/0" },
  {
    path: "spec.template.spec.containers[name=api].resources.limits.memory",
    before: "256Mi",
    after: "512Mi",
  },
  {
    path: "spec.template.spec.containers[name=api].resources.requests.cpu",
    before: "100m",
    after: "250m",
  },
  { path: "spec.template.spec.containers[name=api].resources.requests.memory", before: "128Mi" },
]

const preview = (over: Partial<Schemas["Preview"]> = {}): Schemas["Preview"] => ({
  rollouts: [],
  databases: [],
  domains: [],
  fit: { status: "notNeeded", pods: 0, cpuMillicores: 0, memoryBytes: 0 },
  dataLoss: [],
  removals: [],
  unreadable: [],
  ...over,
})

const plan = (over: Partial<Plan> = {}): Plan => ({
  objects: [],
  impact: [],
  warnings: [],
  ...over,
})

const pods =
  (known: Record<string, Pods>): PodsOf =>
  (workload) =>
    known[workload.name]

group("segments", () => {
  test("keep a keyed entry whole, dots and all", () => {
    expect(segments("a.b[name=x.y].c")).toStrictEqual(["a", "b[name=x.y]", "c"])
    expect(segments("spec.replicas")).toStrictEqual(["spec", "replicas"])
  })
})

group("editGroups", () => {
  test("none for a plan that changes nothing", () => {
    expect(editGroups(t, plan())).toStrictEqual([])
  })

  test("one variable is one row, its value before and after", () => {
    const [variables] = editGroups(
      t,
      plan({ objects: [object(api, [{ path: env("A", "value"), before: "1", after: "2" }])] }),
    )
    expect(variables?.part).toBe("variables")
    expect(variables?.rows).toStrictEqual([{ kind: "changed", name: "A", before: "1", after: "2" }])
  })

  test("by part in the order of the tabs, removed rows first", () => {
    const groups = editGroups(t, plan({ objects: [object(api, apiChanges)] }))
    expect(groups.map((g) => g.part)).toStrictEqual(["variables", "compute"])
    expect(groups[0]?.object).toBe(undefined)
    expect(groups[0]?.rows).toStrictEqual([
      { kind: "removed", name: "REDIS_URL", before: "redis://redis:6379/0", after: undefined },
      { kind: "changed", name: "LOG_LEVEL", before: "info", after: "debug" },
      { kind: "added", name: "FOO", before: undefined, after: "bar" },
    ])
    expect(groups[1]?.rows.map((r) => [r.kind, r.label, r.before, r.after])).toStrictEqual([
      ["removed", "form.memoryRequest", "128Mi", undefined],
      ["changed", "form.replicas", "3", "5"],
      ["changed", "form.memoryLimit", "256Mi", "512Mi"],
      ["changed", "form.cpuRequest", "100m", "250m"],
    ])
  })

  test("a Secret's values stay hidden as the server sent them", () => {
    const secret = object({ kind: "Secret", name: "keys" }, [
      { path: "data.TOKEN", before: "(hidden)", after: "(hidden)" },
    ])
    expect(editGroups(t, plan({ objects: [secret] }))[0]?.rows[0]).toStrictEqual({
      kind: "changed",
      label: undefined,
      name: "TOKEN",
      before: "(hidden)",
      after: "(hidden)",
    })
  })

  test("a created object is one row, and objects are named when there are several", () => {
    const created = object({ kind: "Ingress", name: "shop" }, [], { beforeYaml: undefined })
    const groups = editGroups(t, plan({ objects: [object(api, apiChanges), created] }))
    expect(groups.map((g) => [g.object, g.part])).toStrictEqual([
      ["Deployment api", "variables"],
      ["Deployment api", "compute"],
      ["Ingress shop", "networking"],
    ])
    expect(groups[2]?.rows).toStrictEqual([
      { kind: "added", label: "Ingress", name: "shop", after: "plan.create", before: undefined },
    ])
  })

  test("a restart's annotation is no row", () => {
    const restarted = object(api, [
      {
        path: "spec.template.metadata.annotations.kubectl.kubernetes.io/restartedAt",
        after: "now",
      },
    ])
    expect(editGroups(t, plan({ objects: [restarted] }))).toStrictEqual([])
  })
})

const names = Array.from({ length: 45 }, (_, i) => `svc-${String(i).padStart(2, "0")}`)
const workload = (name: string) => ({ kind: "Deployment" as const, name })
/** 3 single-replica StatefulSets with downtime, 24 single replicas, 18 with three. */
const known = Object.fromEntries(
  names.map((name, i) => [name, i < 27 ? { desired: 1, ready: 1 } : { desired: 3, ready: 3 }]),
)
const rollouts = names.map((name, i) => ({
  workload: workload(name),
  actions: ["roll" as const],
  downtime: i < 3,
  strategy: "RollingUpdate",
}))

group("outcomeGroups", () => {
  test("none for no change", () => {
    expect(outcomeGroups(t, [], plan(), pods({}))).toStrictEqual([])
  })

  test("one restart is one open group", () => {
    const groups = outcomeGroups(
      t,
      [{ op: "restart", workload: api }],
      plan(),
      pods({ api: { desired: 3, ready: 3 } }),
    )
    expect(groups).toStrictEqual([
      { outcome: "up", open: true, rows: [{ workload: api, replicas: "3/3", effect: undefined }] },
    ])
  })

  test("45 restarts, exceptions first and open, the rest folded", () => {
    const groups = outcomeGroups(
      t,
      names.map((name) => ({ op: "restart" as const, workload: workload(name) })),
      plan({ preview: preview({ rollouts }) }),
      pods(known),
      (w) => (w.name < "svc-03" ? ["/data"] : []),
    )
    expect(groups.map((g) => [g.outcome, g.rows.length, g.open])).toStrictEqual([
      ["down", 3, true],
      ["single", 24, false],
      ["up", 18, false],
    ])
    expect(groups[0]?.rows[0]?.effect).toBe("plan.effect.dataStays")
    expect(groups[2]?.rows[0]?.effect).toBe(undefined)
  })

  test("pausing every service stops them all", () => {
    const groups = outcomeGroups(
      t,
      names.map((name) => ({ op: "scale" as const, workload: workload(name), replicas: 0 })),
      plan(),
      pods(known),
    )
    expect(groups.map((g) => [g.outcome, g.rows.length, g.open])).toStrictEqual([
      ["stops", 45, true],
    ])
    expect(groups[0]?.rows[0]?.replicas).toBe("1 → 0")
  })
})

group("bounded", () => {
  test("lists up to four names whole, more as three and a count", () => {
    expect(bounded(english, []).text).toBe("")
    expect(bounded(english, ["a"]).text).toBe("a")
    expect(bounded(english, ["a", "b", "c", "d"])).toStrictEqual({
      text: "a, b, c, and d",
      rest: [],
    })
    const many = bounded(english, names)
    expect(many.text).toBe("svc-00, svc-01, svc-02, and 42 more")
    expect(many.rest).toStrictEqual(names.slice(3))
  })
})

const envEdit = (n: number): Schemas["Change"] => ({
  op: "env",
  workload: api,
  changes: Array.from({ length: n }, (_, i) => ({ op: "set", name: `V${i}`, value: "x" })),
})

group("describe at 1, 5 and 45 changes", () => {
  test("one edit is a sentence", () => {
    const d = describe(t, { changes: [envEdit(1)], service: api }, plan(), pods({}))
    expect(d.lines).toStrictEqual(["confirm.env(, , common.list([V0]), api)"])
    expect(d.facts).toBe(undefined)
    expect(d.edits).toBe(undefined)
  })

  test("five edits still read as one sentence", () => {
    const d = describe(t, { changes: [envEdit(5)], service: api }, plan(), pods({}))
    expect(d.lines).toHaveLength(1)
    expect(d.facts).toBe(undefined)
  })

  test("45 edits become rows under a summary, and the variables' sentence goes", () => {
    const changes = Array.from({ length: 45 }, (_, i) => ({
      path: env(`V${i}`, "value"),
      after: "x",
    }))
    const d = describe(
      t,
      { changes: [envEdit(45), { op: "scale", workload: api, replicas: 5 }], service: api },
      plan({
        objects: [object(api, [{ path: "spec.replicas", before: "3", after: "5" }, ...changes])],
        preview: preview({
          rollouts: [
            {
              workload: api,
              actions: ["roll", "scale"],
              downtime: false,
              fromReplicas: 3,
              replicas: 5,
            },
          ],
          fit: { status: "fits", pods: 2, cpuMillicores: 0, memoryBytes: 0 },
        }),
      }),
      pods({ api: { desired: 3, ready: 3 } }),
    )
    expect(d.lines).toStrictEqual([])
    expect(d.facts?.map((f) => f.text)).toStrictEqual([
      "plan.fact.changed(1)",
      "plan.fact.added(45)",
      "plan.fact.replicas(3, 5)",
      "plan.fact.staysUp",
      "plan.fact.fits",
    ])
    expect(d.edits?.map((g) => [g.part, g.rows.length])).toStrictEqual([
      ["variables", 45],
      ["compute", 1],
    ])
    expect(d.said).toContainEqual(api)
    expect(d.action).toBe("action.applyMany(46)")
  })

  test("restarting 45 services says each verdict once, with counts", () => {
    const d = describe(
      t,
      { changes: names.map((name) => ({ op: "restart" as const, workload: workload(name) })) },
      plan({ preview: preview({ rollouts }) }),
      pods(known),
      ["fewPods"],
    )
    expect(d.lines).toStrictEqual([])
    expect(d.facts?.map((f) => f.text)).toStrictEqual([
      "plan.outcome.down.count(3)",
      "plan.outcome.single.count(24)",
      "plan.outcome.up.count(18)",
    ])
    const risk = "risk.fewPods(common.list([svc-00, svc-01, svc-02, common.more(24)]), 27)"
    expect(d.risk).toBe(`common.sentences([${risk}])`)
    expect(d.more[risk]).toHaveLength(27)
    expect(d.action).toBe("action.restartMany(45)")
  })

  test("pausing 45 services names the verb, the risk once and the way back", () => {
    const changes = names.map((name) => ({
      op: "scale" as const,
      workload: workload(name),
      replicas: 0,
    }))
    const d = describe(t, { changes }, plan(), pods(known), ["scaleToZero"])
    expect(d.action).toBe("action.pauseMany(45)")
    expect(d.risk).toStartWith("common.sentences([risk.scaleToZero(")
    expect(d.cost).toBe("common.sentences([plan.cost.undo])")
    expect(d.facts?.map((f) => f.text)).toStrictEqual(["plan.outcome.stops.count(45)"])
    expect(undoTitle(t, changes, changes)).toBe("undo.scaleUp(45)")
  })

  test("restarting a StatefulSet says its data stays on its volume", () => {
    const db = { kind: "StatefulSet" as const, name: "db" }
    const d = describe(
      t,
      { changes: [{ op: "restart", workload: db }] },
      plan(),
      pods({ db: { desired: 1, ready: 1 } }),
      ["fewPods"],
      () => ["/data"],
    )
    expect(d.cost).toBe("common.sentences([plan.cost.dataStaysOne(db, common.list([/data]))])")
  })

  test("resources say what each value was", () => {
    const d = describe(
      english,
      {
        changes: [
          {
            op: "resources",
            workload: api,
            requests: { cpu: "250m" },
            limits: { memory: "512Mi" },
          },
        ],
      },
      plan({ objects: [object(api, apiChanges)] }),
      pods({}),
    )
    expect(d.lines[0]).toBe(
      "Reserved CPU 100m → 250m, Reserved memory 128Mi → Not set, Maximum memory 256Mi → 512Mi.",
    )
  })
})
