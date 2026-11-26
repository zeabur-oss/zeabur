import { describe, expect, test } from "bun:test"

import type { Build } from "@/model/builds"
import {
  createUploadService,
  newUploadService,
  uploadProblems,
  uploadSourceChange,
  type UploadService,
} from "@/model/upload-service"

const form = (patch: Partial<UploadService> = {}): UploadService => ({
  ...newUploadService(),
  name: "site",
  port: "3000",
  ...patch,
})

describe("a service from an uploaded folder", () => {
  test("one change names the service, its token request and how to build the folder", () => {
    expect(createUploadService(form(), "u1")).toMatchObject({
      op: "createBuiltService",
      name: "site",
      port: 3000,
      build: { source: { type: "upload", request: "u1" }, method: "railpack" },
    })
  })

  test("the folder inside the upload and the Dockerfile are sent clean, the Dockerfile only for that method", () => {
    const options = { directory: "./apps/web/", dockerfile: "docker/Dockerfile.prod" }
    const railpack = createUploadService(form(options), "u1")
    expect(railpack).toMatchObject({ build: { directory: "apps/web" } })
    expect(railpack).not.toHaveProperty("build.dockerfile")
    expect(createUploadService(form({ ...options, method: "dockerfile" }), "u1")).toMatchObject({
      build: { directory: "apps/web", dockerfile: "docker/Dockerfile.prod" },
    })
  })

  test("a host goes with a port and a public URL, and a worker has neither", () => {
    expect(
      createUploadService(form({ publicUrl: true, host: " a.example.com " }), "u1"),
    ).toMatchObject({
      host: "a.example.com",
    })
    expect(
      createUploadService(form({ port: "", publicUrl: true, host: "a.example.com" }), "u1"),
    ).not.toHaveProperty("port")
    expect(
      createUploadService(form({ port: "", publicUrl: true, host: "a.example.com" }), "u1"),
    ).not.toHaveProperty("host")
  })

  test("a form without a folder, or with a bad name, cannot be reviewed", () => {
    const rules = { taken: ["api"], badEnv: false, badBuildEnv: false }
    expect(uploadProblems(form(), { ...rules, hasFolder: true })).toMatchObject({
      folder: false,
      name: undefined,
    })
    expect(uploadProblems(form(), { ...rules, hasFolder: false }).folder).toBe(true)
    expect(uploadProblems(form({ name: "api" }), { ...rules, hasFolder: true }).name).toBeDefined()
  })
})

describe("building a running service from a folder", () => {
  const workload = { kind: "Deployment", name: "site" } as const
  const none = () => false

  test("an image service gets a Build that waits for the folder, with a Dockerfile when it has one", () => {
    expect(
      uploadSourceChange(undefined, { workload, container: "site", request: "u1" }, none),
    ).toEqual({
      op: "setBuild",
      workload,
      container: "site",
      build: { source: { type: "upload", request: "u1" }, method: "railpack" },
    })
    const found = (path: string) => path === "Dockerfile"
    expect(uploadSourceChange(undefined, { workload, request: "u1" }, found)).toMatchObject({
      build: { method: "dockerfile" },
    })
  })

  test("a Git service's Build reads the folder instead, with its paths and method", () => {
    const build = {
      name: "site",
      builder: "dockerfile",
      gitUrl: "https://github.com/acme/site",
      contextDir: "apps/web",
      dockerfile: "Dockerfile.prod",
    } as Build
    expect(uploadSourceChange(build, { workload, request: "u2" }, none)).toEqual({
      op: "setBuild",
      workload,
      build: {
        source: { type: "upload", request: "u2" },
        method: "dockerfile",
        directory: "apps/web",
        dockerfile: "Dockerfile.prod",
      },
    })
  })
})
