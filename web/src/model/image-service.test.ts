import { describe, expect, test } from "bun:test"

import {
  builtName,
  createImageService,
  hasTag,
  hostPlaceholder,
  hostProblem,
  imageChange,
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
    expect(suggestedHost("web", "untitled-5", "172.238.20.4.sslip.io")).toBe(
      "web-untitled-5.172-238-20-4.sslip.io",
    )
  })

  test("a built service is named after its source, else web, so the host shows from the start", () => {
    expect(builtName("", [])).toBe("web")
    expect(builtName("", ["web"])).toBe("web-2")
    expect(builtName("blog", [])).toBe("blog")
    expect(suggestedHost(builtName("", []), "shop", "apps.example.com")).toBe(
      "web-shop.apps.example.com",
    )
  })

  test("an empty host field shows the shape the suffix gives, or the example without one", () => {
    expect(hostPlaceholder("shop", "apps.example.com")).toBe("<name>-shop.apps.example.com")
    expect(hostPlaceholder("shop", undefined)).toBe("web.example.com")
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

describe("the change", () => {
  test("only an image is a worker: no port, no host", () => {
    const change = imageChange(form({ publicUrl: true, host: "web.example.com" }))
    expect(change.port).toBeUndefined()
    expect(change.host).toBeUndefined()
    expect(change.image).toBe("ghcr.io/acme/web:1.4")
  })

  test("a port carries the more ports; a public URL its host", () => {
    const change = imageChange(
      form({ port: "8080", morePorts: [9090], publicUrl: true, host: " web.example.com " }),
    )
    expect(change.port).toBe(8080)
    expect(change.morePorts).toEqual([9090])
    expect(change.host).toBe("web.example.com")
    expect(imageChange(form({ port: "8080" })).host).toBeUndefined()
  })

  test("a Secret key stays a reference", () => {
    const variables = [
      { name: "A", value: "1" },
      { name: "DATABASE_URL", valueFrom: { secretName: "postgres-app", key: "uri" } },
    ]
    expect(imageChange(form({ variables })).variables).toEqual(variables)
  })

  test("the form is one createImageService change", () => {
    expect(createImageService(form({ port: "80" }))).toEqual([
      { op: "createImageService", ...imageChange(form({ port: "80" })) },
    ])
  })

  test("a new image service is Compatible: root may run", () => {
    expect(newImageService().security).toBe("baseline")
    expect(imageChange(form({})).security).toBe("baseline")
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
