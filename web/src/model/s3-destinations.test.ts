import { describe, expect, test } from "bun:test"

import {
  AWS_KEYS,
  destinationForm,
  destinationProblems,
  fromDestination,
  keysSecret,
  needsKeys,
} from "@/model/s3-destinations"

const r2 = {
  name: "r2",
  endpoint: "https://r2.test",
  bucket: "backups",
  region: "auto",
  prefix: "prod",
  origin: { kind: "zeabur" },
} as never

describe("S3 destinations", () => {
  test("a destination fills the fields under its own folder and leaves the keys to the plan", () => {
    const target = { ...fromDestination(r2, "shop"), accessKey: "", secretKey: "" }
    expect(target).toMatchObject({ destination: "r2", bucket: "backups", path: "prod/shop" })
    expect(needsKeys({ endpoint: "", bucket: "", path: "", region: "", ...target })).toBe(false)
  })

  test("the keys are copied from the destination, or typed", () => {
    const target = {
      endpoint: "",
      bucket: "",
      path: "",
      region: "",
      accessKey: " id ",
      secretKey: "k",
    }
    expect(
      keysSecret("db-s3", { ...target, destination: "r2" }, AWS_KEYS, { R: "x" }),
    ).toMatchObject({
      op: "copyS3Keys",
      destination: "r2",
      object: {
        apiVersion: "v1",
        kind: "Secret",
        metadata: { name: "db-s3" },
        type: "Opaque",
        stringData: { R: "x" },
      },
      ...AWS_KEYS,
    })
    expect(keysSecret("db-s3", target, AWS_KEYS)).toMatchObject({
      op: "create",
      object: { stringData: { AWS_ACCESS_KEY_ID: "id", AWS_SECRET_ACCESS_KEY: "k" } },
    })
  })

  test("a new destination needs both keys and a free name; an edit takes both or neither", () => {
    const form = {
      ...destinationForm(),
      name: "r2",
      endpoint: "https://r2.test",
      bucket: "backups",
    }
    expect(destinationProblems(form, ["r2"], false)).toEqual(["taken", "keys"])
    expect(destinationProblems(form, ["r2"], true)).toEqual([])
    expect(destinationProblems({ ...form, accessKeyId: "id" }, [], true)).toEqual(["keys"])
  })
})
