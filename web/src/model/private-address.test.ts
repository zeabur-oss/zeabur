import { describe, expect, test } from "bun:test"

import type { Build } from "@/model/builds"
import {
  exposeCommand,
  imagePorts,
  exposureOf,
  likelyPort,
  privateAddress,
  privateService,
  serviceNameOf,
} from "@/model/private-address"
import type { Service } from "@/model/service"

const workload = (kind: string, fields: object = {}) =>
  ({
    kind,
    name: "web",
    namespace: "shop",
    containers: [{ name: "app" }],
    selector: { matchLabels: { app: "web" } },
    ...fields,
  }) as unknown as Service

const container = (fields: object) => ({ name: "app", ...fields })

describe("a private address for a service with none", () => {
  test.each(["Deployment", "StatefulSet", "DaemonSet"])(
    "is a ClusterIP Service named and labelled like the %s, so deleting the service deletes it",
    (kind) => {
      const service = workload(kind)
      const exposure = exposureOf(service)
      if (!exposure.can) throw new Error("refused")
      expect(privateService(service, exposure.selector, 8080)).toStrictEqual({
        apiVersion: "v1",
        kind: "Service",
        metadata: { name: "web", labels: { "app.kubernetes.io/name": "web" } },
        spec: {
          type: "ClusterIP",
          selector: { app: "web" },
          ports: [{ port: 8080, targetPort: 8080 }],
        },
      })
      expect(privateAddress(service, 8080)).toBe("web.shop.svc:8080")
    },
  )

  test("is named as a Service may be when the workload's name is a DNS subdomain", () => {
    const dotted = workload("StatefulSet", { name: "api.worker" })
    expect(privateService(dotted, { app: "web" }, 80).metadata).toStrictEqual({
      name: "api-worker",
      labels: { "app.kubernetes.io/name": "api.worker" },
    })
    expect(privateAddress(dotted, 80)).toBe("api-worker.shop.svc:80")
    expect(exposeCommand(workload("Deployment", { name: "api.worker" }), { app: "web" }, 80)).toBe(
      "kubectl -n shop expose deployment api.worker --name=api-worker --port=80",
    )
    expect(serviceNameOf(workload("Deployment", { name: "1st" }))).toBe("svc-1st")

    const long = workload("Deployment", { name: `${"a".repeat(62)}.b` })
    expect(serviceNameOf(long)).toBe("a".repeat(62))
    expect(privateService(long, { app: "web" }, 80).metadata).toStrictEqual({
      name: "a".repeat(62),
    })
  })

  test("carries every port the containers declare, the chosen one first, each named", () => {
    const service = workload("StatefulSet", {
      containers: [
        { name: "setup", init: true, ports: [{ port: 1 }] },
        container({
          ports: [
            { name: "metrics", port: 9090 },
            { port: 5353, protocol: "UDP" },
          ],
        }),
        { name: "proxy", ports: [{ port: 8080 }] },
      ],
    })
    expect(privateService(service, { app: "web" }, 8080).spec.ports).toStrictEqual([
      { name: "port-8080", port: 8080, targetPort: 8080 },
      { name: "metrics", port: 9090, targetPort: 9090 },
      { name: "udp-5353", port: 5353, targetPort: 5353, protocol: "UDP" },
    ])
  })

  test("names a port apart from the names the containers declare", () => {
    const service = workload("Deployment", {
      containers: [
        container({
          ports: [
            { port: 8080 },
            { name: "port-8080", port: 9090 },
            { name: "port-8080-2", port: 9091 },
          ],
        }),
      ],
    })
    expect(privateService(service, { app: "web" }, 8080).spec.ports.map((p) => p.name)).toEqual([
      "port-8080-3",
      "port-8080",
      "port-8080-2",
    ])
  })

  test("adds a typed port the containers do not declare", () => {
    const service = workload("Deployment", {
      containers: [container({ ports: [{ port: 80 }] })],
    })
    expect(privateService(service, { app: "web" }, 3000).spec.ports.map((p) => p.port)).toEqual([
      3000, 80,
    ])
  })

  test("is refused when the selector has matchExpressions, and for a Job", () => {
    const expressions = workload("Deployment", {
      selector: { matchLabels: { app: "web" }, expressions: true },
    })
    expect(exposureOf(expressions)).toStrictEqual({
      can: false,
      why: "expressions",
    })
    expect(exposureOf(workload("Job", { selector: null }))).toStrictEqual({
      can: false,
      why: "kind",
    })
  })
})

describe("the port it suggests", () => {
  test("is the readiness probe's, by number or by the name of a declared port", () => {
    const probe = (port: string) =>
      workload("Deployment", {
        containers: [
          container({
            probes: { readiness: { handler: "HTTP", port } },
            ports: [{ port: 9090 }, { name: "http", port: 8080 }],
          }),
        ],
      })
    expect(likelyPort(probe("3000"))).toBe(3000)
    expect(likelyPort(probe("http"))).toBe(8080)
  })

  test("is else the first TCP port declared, else none", () => {
    const ports = (list: object[]) =>
      workload("Deployment", { containers: [container({ ports: list })] })
    expect(likelyPort(ports([{ port: 53, protocol: "UDP" }, { port: 8080 }]))).toBe(8080)
    expect(likelyPort(ports([]))).toBeUndefined()
  })
})

describe("the kubectl command", () => {
  test("is kubectl expose for a Deployment with one port", () => {
    expect(exposeCommand(workload("Deployment"), { app: "web" }, 8080)).toBe(
      "kubectl -n shop expose deployment web --port=8080",
    )
    expect(exposeCommand(workload("Deployment"), { app: "web" })).toBe(
      "kubectl -n shop expose deployment web --port=<port>",
    )
  })

  test("applies the same Service for a StatefulSet, which kubectl expose refuses", () => {
    expect(exposeCommand(workload("StatefulSet"), { app: "web" }, 5432)).toBe(
      [
        "kubectl -n shop apply -f - <<'EOF'",
        "apiVersion: v1",
        "kind: Service",
        "metadata:",
        "  name: web",
        "  labels:",
        "    app.kubernetes.io/name: web",
        "spec:",
        "  type: ClusterIP",
        "  selector:",
        "    app: web",
        "  ports:",
        "  - port: 5432",
        "    targetPort: 5432",
        "EOF",
      ].join("\n"),
    )
    expect(exposeCommand(workload("DaemonSet"), { app: "web" })).toContain("port: <port>")
  })
})

describe("the ports the image of a Build declares", () => {
  const built = (exposedPorts?: string[]): Build =>
    ({ name: "web", repository: "zot.zot.svc:5000/shop/web", exposedPorts }) as unknown as Build
  const running = (fields: object = {}) =>
    workload("Deployment", {
      containers: [container({ image: "zot.zot.svc:5000/shop/web:main-1-aaaaaaa", ...fields })],
    })

  test("are suggested when the service does not use them yet, TCP only", () => {
    expect(imagePorts(running(), [built(["3000/tcp", "53/udp"])])).toStrictEqual([3000])
    expect(imagePorts(running(), [built(["8080", "3000/TCP"])])).toStrictEqual([3000, 8080])
  })

  test("are left out when a container declares them or a Service has them", () => {
    const declared = running({ ports: [{ port: 3000 }] })
    expect(imagePorts(declared, [built(["3000/tcp", "9000/tcp"])])).toStrictEqual([9000])
    const endpoint = { ports: [{ port: 9000 }] } as never
    expect(imagePorts(declared, [built(["3000/tcp", "9000/tcp"])], [endpoint])).toStrictEqual([])
  })

  test("are none when the image declares none or no Build made the image", () => {
    expect(imagePorts(running(), [built()])).toStrictEqual([])
    expect(imagePorts(running(), [])).toStrictEqual([])
    const other = { ...built(["3000/tcp"]), repository: "zot.zot.svc:5000/shop/api" } as Build
    expect(imagePorts(running(), [other])).toStrictEqual([])
  })

  test("ignore an init container's image", () => {
    const service = workload("Deployment", {
      containers: [
        container({ name: "setup", init: true, image: "zot.zot.svc:5000/shop/web:main-1-aaaaaaa" }),
        container({ image: "busybox" }),
      ],
    })
    expect(imagePorts(service, [built(["3000/tcp"])])).toStrictEqual([])
  })
})
