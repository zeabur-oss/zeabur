import { describe, expect, test } from "bun:test"

import {
  newUploadService,
  uploadObjects,
  uploadProblems,
  UPLOAD_TOKEN_REQUEST,
  type UploadService,
} from "@/model/upload-service"

const form = (patch: Partial<UploadService> = {}): UploadService => ({
  ...newUploadService(),
  name: "site",
  port: "3000",
  ...patch,
})

type Object = Record<string, any>
const build = (objects: Object[]) => objects.find((o) => o.kind === "Build") as Object

describe("a service from an uploaded folder", () => {
  test("its Build builds an archive and asks for the upload token as it is made", () => {
    const objects = uploadObjects(form(), "shop", "zot.zot.svc:5000", {}, "u1")
    const made = build(objects)
    expect(made.spec.source).toEqual({ archive: {} })
    expect(made.metadata.annotations).toEqual({ [UPLOAD_TOKEN_REQUEST]: "u1" })
    expect(made.spec.output.image).toBe("zot.zot.svc:5000/shop/site")
    const deployment = objects.find((o) => o.kind === "Deployment") as Object
    expect(deployment.spec.template.spec.containers[0].image).toBe(
      "zot.zot.svc:5000/shop/site:pending",
    )
    expect(deployment.metadata.annotations["keel.sh/policy"]).toBe("force")
  })

  test("the folder inside the upload and the Dockerfile are the archive's, the Dockerfile only for that method", () => {
    const options = { directory: "./apps/web/", dockerfile: "docker/Dockerfile.prod" }
    const railpack = build(uploadObjects(form(options), "shop", "r:5000", {}, "u1"))
    expect(railpack.spec.source.archive).toEqual({ contextDir: "apps/web" })
    const docker = build(
      uploadObjects(form({ ...options, method: "dockerfile" }), "shop", "r:5000", {}, "u1"),
    )
    expect(docker.spec.source.archive).toEqual({
      contextDir: "apps/web",
      dockerfile: "docker/Dockerfile.prod",
    })
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
