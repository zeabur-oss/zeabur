import { describe as group, test, expect } from "bun:test"

import type { Schemas } from "@/api/client"
import { actionOf, describe, failureOf, replaced } from "@/components/plan/describe"
import { translator, type Translate } from "@/i18n"
import { en } from "@/i18n/en"
import { zhTW } from "@/i18n/zh-TW"
import type { Pods, PodsOf } from "@/model/risk"

// Messages render as key and arguments (`confirm.scale(api, 2, 4)`), so copy edits change no test.
const show = (value: unknown): string =>
  Array.isArray(value) ? `[${value.map(show).join(", ")}]` : String(value)
const t = ((key: string, ...args: unknown[]) =>
  args.length > 0 ? `${key}(${args.map(show).join(", ")})` : key) as unknown as Translate

const english = translator(en)

type Plan = Pick<Schemas["PlanResponse"], "objects" | "impact" | "warnings">
const api = { kind: "Deployment", name: "api" } as const
const web = { kind: "Deployment", name: "web" } as const
const worker = { kind: "Deployment", name: "worker" } as const
const pods =
  (known: Record<string, Pods>): PodsOf =>
  (workload) =>
    known[workload.name]

const templated = (name: string) =>
  ({
    target: { apiVersion: "apps/v1", kind: "Deployment", plural: "deployments", name },
    changes: [{ path: "spec.template.metadata.annotations.restartedAt", after: "now" }],
    owners: [],
    after: {},
    write: { type: "patch", operations: [] },
  }) as unknown as Schemas["PlannedObject"]

const plan = (over: Partial<Plan> = {}): Plan => ({
  objects: [],
  impact: [],
  warnings: [],
  ...over,
})

/** A ConfigMap value set from api's page, read by api (which needs a restart), web and worker. */
const configMapEdit = (translate: Translate) =>
  describe(
    translate,
    {
      changes: [
        {
          op: "data",
          kind: "ConfigMap",
          name: "shop-config",
          changes: [{ op: "set", key: "mode", value: "green" }],
        },
      ],
      service: api,
    },
    plan({
      impact: [api, web, worker],
      warnings: [{ code: "restartNeeded", workloads: [api] }],
    }),
    pods({}),
  )

group("describe", () => {
  test("a restart with three ready pods stays up", () => {
    const d = describe(
      t,
      { changes: [{ op: "restart", workload: api }] },
      plan({ objects: [templated("api")], impact: [api] }),
      pods({ api: { desired: 3, ready: 3 } }),
    )
    // Said once: the restart's own sentence, not again as a replaced template.
    expect(d.lines).toStrictEqual(["confirm.rolling(api, 3)"])
    expect(d.action).toBe("action.restart")
  })

  test("a restart of a single pod says it may be down", () => {
    const d = describe(
      t,
      { changes: [{ op: "restart", workload: api }] },
      plan({ objects: [templated("api")] }),
      pods({ api: { desired: 1, ready: 1 } }),
      ["fewPods"],
    )
    expect(d.lines).toStrictEqual(["confirm.restartSingle(api)"])
    expect(d.consequence).toBe(undefined)
  })

  test("a ConfigMap change names the readers that keep the old value and the others it affects", () => {
    const d = configMapEdit(t)
    expect(d.lines).toStrictEqual([
      "confirm.dataSet(common.list([mode]), ConfigMap, shop-config)",
      "confirm.readersKeepOld(common.list([api]), shop-config, 1)",
      "confirm.alsoReads(common.list([web, worker]), shop-config, 2)",
    ])
    expect(d.action).toBe("action.saveValues")
  })

  test("in English, names are joined into a list and counts agree with them", () => {
    expect(configMapEdit(english).lines).toStrictEqual([
      "Set mode in ConfigMap shop-config.",
      "api reads shop-config and keeps the old value until restarted.",
      "Also affects web and worker (they read shop-config).",
    ])
  })

  test("variables replace the pods of their service", () => {
    const d = describe(
      t,
      {
        changes: [
          {
            op: "env",
            workload: api,
            changes: [
              { op: "set", name: "A", value: "1" },
              { op: "remove", name: "B" },
            ],
          },
        ],
      },
      plan({ objects: [templated("api")], impact: [api] }),
      pods({ api: { desired: 2, ready: 2 } }),
    )
    expect(d.lines).toStrictEqual([
      "confirm.envBoth(common.list([A]), common.list([B]), api)",
      "confirm.rolling(api, 2)",
    ])
    expect(d.action).toBe("action.saveVariables")
  })

  test("a rollback says what is not rolled back, and its button names the revision", () => {
    const d = describe(
      t,
      { changes: [{ op: "rollback", workload: api, revision: 5 }] },
      plan({
        objects: [templated("api")],
        warnings: [{ code: "configNotRolledBack", workload: api }],
      }),
      pods({ api: { desired: 1, ready: 1 } }),
      ["rollback"],
    )
    expect(d.lines).toStrictEqual([
      "confirm.rollback(5)",
      "confirm.notRolledBack",
      "confirm.rollingSingle(api)",
    ])
    expect(d.consequence).toBe(undefined)
    expect(d.action).toBe("action.rollback(5)")
  })

  test("scaling names both counts, or the new one when the old is unknown", () => {
    const changes = [{ op: "scale", workload: api, replicas: 4 } as const]
    const d = describe(t, { changes }, plan(), pods({ api: { desired: 2, ready: 2 } }))
    expect(d.lines).toStrictEqual(["confirm.scale(api, 2, 4)"])
    expect(d.action).toBe("action.scale(4)")
    expect(describe(t, { changes }, plan(), pods({})).lines).toStrictEqual([
      "confirm.scaleTo(api, 4)",
    ])
  })

  test("growing a volume states that it cannot shrink", () => {
    const d = describe(
      t,
      { changes: [{ op: "expandVolume", claim: "data", size: "20Gi" }] },
      plan(),
      pods({}),
      ["storage"],
    )
    // Said once, as what happens; the title already says the new size.
    expect(d.lines).toStrictEqual(["confirm.noShrink"])
    expect(d.consequence).toBe(undefined)
    expect(d.action).toBe("action.grow(20Gi)")
  })

  test("a probe says what it does, with its port and Kubernetes' default timing", () => {
    const d = describe(
      t,
      {
        changes: [
          {
            op: "probe",
            workload: api,
            probe: "readiness",
            set: { handler: "TCPSocket", port: 8080 },
          },
        ],
      },
      plan(),
      pods({}),
    )
    // Every 10 seconds and 3 failures in a row when unset.
    expect(d.lines).toStrictEqual(["confirm.probeSet.readiness(confirm.check.tcp(8080), 10, 3)"])
    expect(d.action).toBe("action.saveHealthChecks")
  })

  test("a liveness probe names its failures, and a removed probe what goes", () => {
    const d = describe(
      t,
      {
        changes: [
          {
            op: "probe",
            workload: api,
            probe: "liveness",
            set: { handler: "HTTPGet", port: 80, path: "/healthz", failureThreshold: 5 },
          },
          { op: "probe", workload: api, probe: "readiness" },
        ],
      },
      plan(),
      pods({}),
    )
    expect(d.lines).toStrictEqual([
      "confirm.probeSet.liveness(confirm.check.http(80, /healthz), 10, 5)",
      "confirm.probeRemoved.readiness",
    ])
  })

  test("resources name the values they set", () => {
    const d = describe(
      t,
      {
        changes: [
          {
            op: "resources",
            workload: api,
            requests: { cpu: "100m", memory: "128Mi" },
            limits: { memory: "256Mi" },
          },
        ],
      },
      plan(),
      pods({}),
    )
    expect(d.lines).toStrictEqual([
      "confirm.resources([[form.cpuRequest, 100m], [form.memoryRequest, 128Mi], [form.memoryLimit, 256Mi]])",
    ])
  })

  test("removing a domain says what stops, once", () => {
    const d = describe(
      t,
      {
        changes: [
          {
            op: "removeDomain",
            hostname: "shop.example.com",
            route: { kind: "Ingress", name: "shop" },
          },
        ],
      },
      plan(),
      pods({}),
      ["removesDomain"],
    )
    expect(d.lines).toStrictEqual(["confirm.removeDomain(shop.example.com)"])
    expect(d.consequence).toBe(undefined)
  })

  test("an isolating policy states its consequence first, for the service it was asked from", () => {
    const policy = {
      kind: "NetworkPolicy",
      metadata: { name: "zeabur-isolate-api" },
      spec: { policyTypes: ["Ingress", "Egress"] },
    } as unknown as Record<string, never>
    const d = describe(
      t,
      { changes: [{ op: "create", object: policy }], service: api },
      plan(),
      pods({}),
      ["isolates"],
    )
    expect(d.consequence).toBe("confirm.isolates(api)")
    expect(d.lines).toStrictEqual(["confirm.create(NetworkPolicy, zeabur-isolate-api)"])
    expect(d.action).toBe("action.isolate")
  })
})

group("deletion", () => {
  test("deleting a Deployment says it cannot be undone, and its button deletes", () => {
    const d = describe(
      t,
      {
        changes: [
          {
            op: "delete",
            resource: { group: "apps", version: "v1", resource: "deployments" },
            name: "web",
          },
        ],
      },
      plan({ impact: [web] }),
      pods({}),
      ["deletes"],
    )
    expect(d.lines).toStrictEqual(["confirm.delete(web)"])
    expect(d.consequence).toBe(undefined)
    expect(d.action).toBe("action.delete")
  })
})

group("actionOf", () => {
  test("the compute form saves; mixed kinds apply", () => {
    expect(
      actionOf(t, {
        changes: [
          { op: "scale", workload: api, replicas: 2 },
          { op: "resources", workload: api },
        ],
      }),
    ).toBe("action.save")
    expect(
      actionOf(t, {
        changes: [
          { op: "restart", workload: api },
          { op: "addDomain", workload: api, hostname: "a" },
        ],
      }),
    ).toBe("plan.apply")
  })

  test("several volume changes save the volumes, even of one kind", () => {
    const add = (mountPath: string): Schemas["Change"] => ({
      op: "addVolume",
      workload: api,
      volume: { mountPath, size: "1Gi" },
    })
    expect(actionOf(t, { changes: [add("/a")] })).toBe("action.addVolume")
    expect(actionOf(t, { changes: [add("/a"), add("/b")] })).toBe("action.saveVolumes")
    expect(
      actionOf(t, {
        changes: [add("/a"), { op: "removeVolume", workload: api, mountPath: "/b" }],
      }),
    ).toBe("action.saveVolumes")
  })
})

group("replaced", () => {
  test("only workloads whose template changes", () => {
    const configMap = {
      ...templated("shop-config"),
      target: { apiVersion: "v1", kind: "ConfigMap", plural: "configmaps", name: "shop-config" },
    }
    expect(replaced(plan({ objects: [templated("api"), configMap] }))).toStrictEqual([api])
  })
})

group("failureOf", () => {
  const problem = (
    kind: Schemas["Kind"],
    extra: Partial<Schemas["Problem"]> = {},
  ): Schemas["Problem"] => ({
    type: `/problems/${kind}`,
    title: "",
    status: 400,
    detail: "",
    fix: en[`problem.fix.${kind}`],
    kind,
    summary: { key: `problem.summary.${kind}` },
    ...extra,
  })

  test("a conflict says what changed and what to do, and asks to review again", () => {
    const f = failureOf(
      english,
      "en",
      problem("conflict", {
        summary: { key: "problem.summary.conflict", params: { name: "web" } },
        fixKey: { key: "problem.conflict.fix.review" },
      }),
    )
    expect(f).toStrictEqual({
      title: "web changed after you reviewed it.",
      description: "Review again to start from its current state.",
      review: true,
    })
  })

  test("an expired plan asks to review again, by its kind; another bad request does not", () => {
    expect(failureOf(english, "en", problem("plan-expired")).review).toBe(true)
    // The wording alone is not an expired plan.
    expect(
      failureOf(english, "en", problem("bad-request", { detail: "the plan expired" })).review,
    ).toBe(false)
  })

  test("a refusal names the refused request from the summary, not kube-apiserver's message", () => {
    const f = failureOf(
      english,
      "en",
      problem("elevation-required", {
        summary: {
          key: "problem.summary.elevation-required",
          params: { verb: "patch", name: "api", resource: "deployments" },
        },
        fixKey: { key: "problem.forbidden.fix.askOrRetry" },
        kubernetes: {
          code: 403,
          reason: "Forbidden",
          message: 'User "ann" cannot patch resource "deployments" in API group "apps"',
        },
      }),
    )
    expect(f.title).toBe("Your everyday rights can't change api.")
    expect(f.description).toBe("Ask an admin, or retry as admin.")
    expect(f.review).toBe(false)
  })

  test("a quota's refusal gives the rule with its numbers, and leaves the evidence out", () => {
    const f = failureOf(
      english,
      "en",
      problem("quota-exceeded", {
        status: 403,
        summary: { key: "problem.summary.quota-exceeded", params: { name: "web" } },
        why: {
          key: "problem.why.quota-exceeded",
          params: { quota: "compute", resource: "requests.cpu", hard: "4", used: "3500m" },
        },
        fixKey: { key: "problem.quotaExceeded.fix.scale", params: { quota: "compute" } },
        evidence: [
          {
            kind: "ResourceQuota",
            name: "compute",
            fieldPath: "status.hard.requests.cpu",
            value: "4",
          },
        ],
      }),
    )
    expect(f.title).toBe("The change to web would go over the project's limit.")
    expect(f.description ?? "").toMatch(/allows 4 of requests\.cpu, and 3500m is already used/)
    expect(f.description ?? "").not.toMatch(/ResourceQuota/)
  })

  test("in another language the toast reads in it", () => {
    const f = failureOf(
      translator(zhTW),
      "zh-TW",
      problem("conflict", { detail: "Already written: x" }),
    )
    expect(f.title).toBe("你預覽之後，它又有了變更。")
  })

  test("a refusal for a managing tool says where to change it, whatever the status and wording", () => {
    // The server's 409 names the tool in `managedBy`; the detail's wording is not read.
    const f = failureOf(t, "en", problem("managed", { detail: "Refused.", managedBy: "Argo CD" }))
    expect(f).toStrictEqual({
      title: "toast.managed(Argo CD)",
      description: "toast.managedGit(Argo CD)",
      review: false,
    })
    expect(failureOf(t, "en", problem("managed", { managedBy: "Helm" })).description).toBe(
      "toast.managedHelm",
    )
    // Prose alone is not a refusal for a tool.
    expect(
      failureOf(
        english,
        "en",
        problem("conflict", { detail: "Deployment api is managed by Flux." }),
      ).title,
    ).toBe("It changed after you reviewed it.")
  })
})

group("terminals", () => {
  test("a debug container says it stays, in plain sentences rather than warnings", () => {
    const said = describe(
      t,
      { changes: [{ op: "debugContainer", pod: "api-1" }] },
      plan({
        warnings: [
          {
            code: "debugContainerStays",
            container: "zeabur-debug-1",
            image: "busybox",
            pod: "api-1",
            target: "api",
          },
        ],
      }),
      pods({}),
    )
    expect(said.lines).toEqual(["confirm.debugContainer(api-1)", "confirm.debugStays"])
    expect(said.action).toBe("action.openDebugTerminal")
  })

  test("a tools pod says as whom it acts, when its credential runs out, and when it goes", () => {
    const expires = new Date(Date.now() + 29.5 * 60_000).toISOString()
    const said = describe(
      t,
      { changes: [{ op: "clusterTools" }] },
      plan({
        warnings: [
          {
            code: "toolsPod",
            admin: false,
            credentialExpiresAt: expires,
            deadlineSeconds: 1800,
            graceSeconds: 60,
            image: "alpine/k8s",
            namespace: "shop",
            person: "alice",
            pod: "zeabur-tools-1",
          },
        ],
      }),
      pods({}),
    )
    expect(said.lines).toEqual([
      "confirm.clusterTools",
      "confirm.toolsActsAs(alice, false)",
      "confirm.toolsCredential(30)",
      "confirm.toolsStops(30, 1)",
    ])
  })
})
