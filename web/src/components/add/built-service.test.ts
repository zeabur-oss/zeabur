import { describe, expect, test } from "bun:test"

import { resolveBuilt, type Choices } from "@/components/add/built-service"
import { newGitService } from "@/model/git-service"
import { parseEnv } from "@/model/image-service"

const untouched: Choices = {
  named: false,
  methoded: false,
  ported: false,
  hosted: false,
  published: false,
  securitySet: false,
}

const resolve = (suffix: string | undefined, choices: Partial<Choices> = {}, publicUrl = true) =>
  resolveBuilt(
    { ...newGitService(), publicUrl },
    { ...untouched, ...choices },
    {
      project: "shop",
      suggested: "web",
      dockerfileFound: false,
      options: { deploy: true, port: true, publicUrl: true, secrets: true, volume: true },
      suffix,
      env: parseEnv(""),
      buildEnv: parseEnv(""),
    },
  )

describe("the public domain of a Git or folder service", () => {
  test("starts on with the host filled in when the cluster has a domain suffix", () => {
    const built = resolve("apps.example.com")
    expect(built.publicUrl).toBe(true)
    expect(built.host).toContain("apps.example.com")
  })

  test("starts off when the cluster has no domain suffix, so nothing asks for a host", () => {
    expect(resolve(undefined).publicUrl).toBe(false)
  })

  test("follows the switch once the person turns it on or off", () => {
    expect(resolve(undefined, { published: true }, true).publicUrl).toBe(true)
    expect(resolve("apps.example.com", { published: true }, false).publicUrl).toBe(false)
  })
})
