import { describe, expect, test } from "bun:test"

import {
  createImageService,
  hasTag,
  hostProblem,
  imageObjects,
  imageOptions,
  imageProblem,
  nameFromImage,
  newImageService,
  parseEnv,
  portProblem,
  rightsOf,
  serviceNameProblem,
  suggestedHost,
  type ImageService,
} from "@/model/image-service"

const form = (patch: Partial<ImageService>): ImageService => ({
  ...newImageService(),
  image: "ghcr.io/acme/web:1.4",
  name: "web",
  ...patch,
})

const kinds = (objects: Record<string, unknown>[]) =>
  objects.map((o) => `${String(o.kind)}/${String((o.metadata as { name: string }).name)}`)

describe("names", () => {
  test("come from the image's last segment, without tag or digest, past those taken", () => {
    expect(nameFromImage("ghcr.io/acme/Web_App:1.4", [])).toBe("web-app")
    expect(nameFromImage("nginx@sha256:abc", ["nginx"])).toBe("nginx-2")
    expect(nameFromImage("redis", ["redis", "redis-2"])).toBe("redis-3")
    expect(nameFromImage("ghcr.io/acme/9lives", [])).toBe("lives")
    expect(nameFromImage("", [])).toBe("app")
  })

  test("a tag or digest is noticed, and a registry port is not a tag", () => {
    expect(hasTag("nginx")).toBe(false)
    expect(hasTag("localhost:5000/web")).toBe(false)
    expect(hasTag("localhost:5000/web:2")).toBe(true)
    expect(hasTag("web@sha256:abc")).toBe(true)
  })

  test("fields are checked", () => {
    expect(imageProblem(" ")).toBe("empty")
    expect(imageProblem("ghcr.io/acme/web:V1")).toBeUndefined()
    expect(imageProblem("web app")).toBe("invalid")
    expect(serviceNameProblem("web", ["web"])).toBe("taken")
    expect(serviceNameProblem("Web", [])).toBe("invalid")
    expect(portProblem("")).toBeUndefined()
    expect(portProblem("8080")).toBeUndefined()
    expect(portProblem("70000")).toBe("invalid")
    expect(hostProblem("web-shop.apps.example.com")).toBeUndefined()
    expect(hostProblem("https://web.example.com")).toBe("invalid")
    expect(suggestedHost("web", "shop", "apps.example.com")).toBe("web-shop.apps.example.com")
    expect(suggestedHost("web", "shop", undefined)).toBe("")
  })
})

describe("variables", () => {
  test("are read from .env lines, quotes and export dropped, the last one kept", () => {
    expect(parseEnv('# c\nexport A=1\nB="two words"\nA=3\nnot a line\n1X=2')).toEqual({
      variables: [
        { name: "B", value: "two words" },
        { name: "A", value: "3" },
      ],
      bad: [5, 6],
    })
  })
})

describe("objects", () => {
  test("only an image is a worker: one Deployment, no Service", () => {
    const objects = imageObjects(form({}), {})
    expect(kinds(objects)).toEqual(["Deployment/web"])
    const container = (objects[0] as any).spec.template.spec.containers[0]
    expect(container).toEqual({
      name: "web",
      image: "ghcr.io/acme/web:1.4",
      resources: { requests: { cpu: "100m", memory: "128Mi" }, limits: { memory: "256Mi" } },
    })
    expect((objects[0] as any).spec.selector.matchLabels).toEqual({
      "app.kubernetes.io/name": "web",
    })
    expect(JSON.stringify(objects)).not.toContain("managed-by")
  })

  test("a port adds a Service and a readiness probe; a public URL an Ingress", () => {
    const objects = imageObjects(
      form({ port: "8080", publicUrl: true, host: "web.example.com" }),
      {},
    )
    expect(kinds(objects)).toEqual(["Deployment/web", "Service/web", "Ingress/web"])
    const ingress = objects[2] as any
    expect(ingress.metadata.annotations).toBeUndefined()
    expect(ingress.spec.tls).toBeUndefined()
    expect(ingress.spec.rules[0].http.paths[0].backend).toEqual({
      service: { name: "web", port: { number: 8080 } },
    })
  })

  test("a named ClusterIssuer adds cert-manager's annotation and a tls block", () => {
    const objects = imageObjects(form({ port: "80", publicUrl: true, host: "web.example.com" }), {
      clusterIssuer: "letsencrypt",
    })
    const ingress = objects.at(-1) as any
    expect(ingress.metadata.annotations).toEqual({
      "cert-manager.io/cluster-issuer": "letsencrypt",
    })
    expect(ingress.spec.tls).toEqual([{ hosts: ["web.example.com"], secretName: "web-tls" }])
  })

  test("variables go in a Secret unless asked not to; a Secret key stays a reference", () => {
    const variables = [
      { name: "A", value: "1" },
      { name: "DATABASE_URL", from: { secret: "postgres-app", key: "uri" } },
    ]
    const kept = imageObjects(form({ variables }), {})
    expect(kinds(kept)).toEqual(["Secret/web-env", "Deployment/web"])
    const container = (kept[1] as any).spec.template.spec.containers[0]
    expect(container.envFrom).toEqual([{ secretRef: { name: "web-env" } }])
    expect(container.env).toEqual([
      { name: "DATABASE_URL", valueFrom: { secretKeyRef: { name: "postgres-app", key: "uri" } } },
    ])
    const plain = imageObjects(form({ variables, secretVariables: false }), {})
    expect(kinds(plain)).toEqual(["Deployment/web"])
    expect((plain[0] as any).spec.template.spec.containers[0].env[0]).toEqual({
      name: "A",
      value: "1",
    })
  })

  test("a volume makes one replica that is recreated, not rolled", () => {
    const objects = imageObjects(
      form({ replicas: 3, volume: { mountPath: "/data", size: "1Gi" } }),
      {},
    )
    expect(kinds(objects)).toEqual(["PersistentVolumeClaim/web-data", "Deployment/web"])
    const spec = (objects[1] as any).spec
    expect(spec.replicas).toBe(1)
    expect(spec.strategy).toEqual({ type: "Recreate" })
    expect(spec.template.spec.volumes[0].persistentVolumeClaim.claimName).toBe("web-data")
  })

  test("a private registry is a dockerconfigjson Secret the pods pull with", () => {
    const objects = imageObjects(
      form({ registry: { server: "ghcr.io", username: "me", password: "pw" } }),
      {},
    )
    const secret = objects[0] as any
    expect(secret.type).toBe("kubernetes.io/dockerconfigjson")
    expect(JSON.parse(secret.stringData[".dockerconfigjson"])).toEqual({
      auths: { "ghcr.io": { username: "me", password: "pw", auth: btoa("me:pw") } },
    })
    expect((objects[1] as any).spec.template.spec.imagePullSecrets).toEqual([
      { name: "web-registry" },
    ])
  })

  test("each object is one create change", () => {
    expect(createImageService(form({ port: "80" }), {}).map((c) => c.op)).toEqual([
      "create",
      "create",
    ])
  })
})

describe("rights", () => {
  test("a denied kind hides its part; unknown offers it", () => {
    const rights = rightsOf({
      clusterScoped: [],
      items: [
        { group: "networking.k8s.io", resource: "ingresses", access: "denied" } as any,
        { group: "", resource: "secrets", access: "unknown" } as any,
      ],
    })
    expect(imageOptions(rights)).toEqual({
      deploy: true,
      port: true,
      publicUrl: false,
      secrets: true,
      volume: true,
    })
  })
})
