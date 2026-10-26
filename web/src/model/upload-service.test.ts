import { describe, expect, test } from "bun:test"

import {
  createUploadService,
  newUploadService,
  uploadProblems,
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
      op: "createUploadService",
      name: "site",
      request: "u1",
      method: "railpack",
      port: 3000,
    })
  })

  test("the folder inside the upload and the Dockerfile are sent clean, the Dockerfile only for that method", () => {
    const options = { directory: "./apps/web/", dockerfile: "docker/Dockerfile.prod" }
    const railpack = createUploadService(form(options), "u1")
    expect(railpack).toMatchObject({ directory: "apps/web" })
    expect(railpack).not.toHaveProperty("dockerfile")
    expect(createUploadService(form({ ...options, method: "dockerfile" }), "u1")).toMatchObject({
      directory: "apps/web",
      dockerfile: "docker/Dockerfile.prod",
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
