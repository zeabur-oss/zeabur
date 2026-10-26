import { describe, expect, test } from "bun:test"

import { pack, TooLarge } from "@/lib/pack/pack"
import type { SourceFile } from "@/lib/pack/source"

const file = (path: string, text = ""): SourceFile => ({ path, data: new Blob([text]) })

/** The entries of a gzip tar: name, type flag and body. */
async function entries(blob: Blob) {
  const raw = new Uint8Array(
    await new Response(blob.stream().pipeThrough(new DecompressionStream("gzip"))).arrayBuffer(),
  )
  const text = new TextDecoder()
  const out: { name: string; type: string; mode: string; body: string }[] = []
  let at = 0
  let long: string | undefined
  while (at + 512 <= raw.length && raw[at] !== 0) {
    const field = (from: number, to: number) =>
      text.decode(raw.slice(at + from, at + to)).replace(/\0.*$/, "")
    const size = parseInt(field(124, 136), 8)
    const body = text.decode(raw.slice(at + 512, at + 512 + size))
    const type = field(156, 157)
    if (type === "x") {
      long = body.replace(/^\d+ path=/, "").replace(/\n$/, "")
    } else {
      out.push({ name: long ?? field(0, 100), type, mode: field(100, 108), body })
      long = undefined
    }
    at += 512 + Math.ceil(size / 512) * 512
  }
  return out
}

describe("pack", () => {
  test("ignore files and .git decide what goes up; text and modes arrive as they were", async () => {
    const packed = await pack([
      file("Dockerfile", "FROM scratch"),
      file("run.sh", "#!/bin/sh\n"),
      file("src/main.go", "package main"),
      file("src/debug.log"),
      file("src/.gitignore", "*.log\n"),
      file(".gitignore", "node_modules/\n/dist\n"),
      file(".dockerignore", "secret.txt\n"),
      file("node_modules/a/index.js"),
      file("dist/out.js"),
      file("lib/dist/keep.js"),
      file("secret.txt"),
      file("lib/secret.txt"),
      file(".git/config"),
    ])
    const found = await entries(packed.blob)
    const files = found.filter((e) => e.type === "0").map((e) => e.name)
    expect(files).toEqual([
      ".dockerignore",
      ".gitignore",
      "Dockerfile",
      "lib/dist/keep.js",
      "lib/secret.txt",
      "run.sh",
      "src/.gitignore",
      "src/main.go",
    ])
    expect(found.find((e) => e.name === "Dockerfile")?.body).toBe("FROM scratch")
    expect(found.find((e) => e.name === "run.sh")?.mode).toBe("0000755")
    expect(found.find((e) => e.name === "Dockerfile")?.mode).toBe("0000644")
    expect(found.filter((e) => e.type === "5").map((e) => e.name)).toEqual([
      "lib/",
      "lib/dist/",
      "src/",
    ])
    expect(packed.left).toBe(5)
  })

  test("a path longer than a tar header holds survives", async () => {
    const path = `${"deep/".repeat(30)}file-with-a-long-name.txt`
    const found = await entries((await pack([file(path, "x")])).blob)
    expect(found.find((e) => e.type === "0")?.name).toBe(path)
  })

  test("a deeper .gitignore can bring a file back; a folder left out keeps its files out", async () => {
    const packed = await pack([
      file(".gitignore", "*.log\nbuild/\n"),
      file("a/.gitignore", "!keep.log\n"),
      file("a/keep.log"),
      file("a/drop.log"),
      file("build/!keep.log"),
      file("build/x/.gitignore", "!*\n"),
      file("build/x/y"),
    ])
    const names = (await entries(packed.blob)).filter((e) => e.type === "0").map((e) => e.name)
    expect(names).toEqual([".gitignore", "a/.gitignore", "a/keep.log"])
  })

  test("a folder past the unpacked limit says what is largest", async () => {
    const big = { path: "node_modules/big.bin", data: { size: 2 * 1024 ** 3 } as Blob }
    const error = await pack([big, file("src/a.ts", "x")]).catch((e: unknown) => e)
    expect(error).toBeInstanceOf(TooLarge)
    expect((error as TooLarge).largest[0]?.name).toBe("node_modules")
  })
})

describe("inspectTarGz", () => {
  test("an archive goes up as it is, counted by its files", async () => {
    const { inspectTarGz } = await import("@/lib/pack/targz")
    const made = await pack([file("a.txt", "one"), file("src/b.txt", "three")])
    const found = await inspectTarGz(made.blob)
    expect(found.blob).toBe(made.blob)
    expect(found.files).toBe(2)
    expect(found.bytes).toBe(8)
  })

  test("something that is not a gzip tar is refused", async () => {
    const { inspectTarGz } = await import("@/lib/pack/targz")
    await expect(inspectTarGz(new Blob(["not an archive"]))).rejects.toMatchObject({
      reason: "invalid",
    })
  })
})
