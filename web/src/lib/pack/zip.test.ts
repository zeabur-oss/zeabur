import { describe, expect, test } from "bun:test"
import { zipSync, strToU8, type Zippable } from "fflate"

import { pack } from "@/lib/pack/pack"
import { filesOfZip, linkStaysInside } from "@/lib/pack/zip"

const unix = (mode: number) => ({ os: 3, attrs: mode << 16 })

const zipOf = (entries: Record<string, [string, number?]>): Blob => {
  const files: Zippable = {}
  for (const [path, [text, mode]] of Object.entries(entries))
    files[path] = [strToU8(text), mode === undefined ? {} : unix(mode)]
  return new Blob([zipSync(files) as Uint8Array<ArrayBuffer>])
}

const FILE = 0o100000
const LINK = 0o120000

/** Name, type flag, mode and link target of each tar entry. */
async function entries(blob: Blob) {
  const raw = new Uint8Array(
    await new Response(blob.stream().pipeThrough(new DecompressionStream("gzip"))).arrayBuffer(),
  )
  const text = new TextDecoder()
  const out: { name: string; type: string; mode: string; link: string }[] = []
  let at = 0
  let pax = ""
  while (at + 512 <= raw.length && raw[at] !== 0) {
    const field = (from: number, to: number) =>
      text.decode(raw.slice(at + from, at + to)).replace(/\0.*$/, "")
    const size = parseInt(field(124, 136), 8)
    const type = field(156, 157)
    if (type === "x") pax = text.decode(raw.slice(at + 512, at + 512 + size))
    else {
      const named = (key: string) => new RegExp(`\\d+ ${key}=(.*)\\n`).exec(pax)?.[1]
      out.push({
        name: named("path") ?? field(0, 100),
        type,
        mode: field(100, 108),
        link: named("linkpath") ?? field(157, 257),
      })
      pax = ""
    }
    at += 512 + Math.ceil(size / 512) * 512
  }
  return out
}

describe("a zip's modes and links", () => {
  test("execute bits and symbolic links survive the repack", async () => {
    const files = await filesOfZip(
      zipOf({
        "app/bin/run": ["echo", FILE | 0o755],
        "app/data.txt": ["x", FILE | 0o644],
        "app/current": ["bin/run", LINK | 0o777],
        "app/script": ["#!/bin/sh\n", FILE | 0o644],
      }),
    )
    const found = await entries((await pack(files)).blob)
    const by = (name: string) => found.find((e) => e.name === name)
    expect(by("bin/run")?.mode).toBe("0000755")
    expect(by("data.txt")?.mode).toBe("0000644")
    expect(by("script")?.mode).toBe("0000644")
    expect(by("current")).toMatchObject({ type: "2", link: "bin/run" })
  })

  test("a zip without Unix modes still marks a shebang file executable", async () => {
    const files = await filesOfZip(zipOf({ "run.sh": ["#!/bin/sh\n"], "a.txt": ["x"] }))
    const found = await entries((await pack(files)).blob)
    expect(found.find((e) => e.name === "run.sh")?.mode).toBe("0000755")
    expect(found.find((e) => e.name === "a.txt")?.mode).toBe("0000644")
  })

  test("a link target longer than a tar header holds survives", async () => {
    const target = `${"d/".repeat(60)}end`
    const files = await filesOfZip(zipOf({ "x/l": [target, LINK | 0o777], "x/d": ["", FILE] }))
    const found = await entries((await pack(files)).blob)
    expect(found.find((e) => e.name === "l")?.link).toBe(target)
  })

  test.each([
    ["an absolute target", { l: ["/etc/passwd", LINK | 0o777] }],
    ["a climbing target", { "a/l": ["../../x", LINK | 0o777], "a/f": ["x", FILE] }],
    ["a file written through a link", { l: [".", LINK | 0o777], "l/x": ["x", FILE] }],
  ] as [string, Record<string, [string, number]>][])("%s is refused", async (_, entries) => {
    await expect(filesOfZip(zipOf({ top: ["", FILE], ...entries }))).rejects.toMatchObject({
      reason: "link",
    })
  })
})

describe("linkStaysInside", () => {
  test("follows Zeabur Builder's rule", () => {
    expect(linkStaysInside("src/main.js", "index.js")).toBe(true)
    expect(linkStaysInside("src/pkg.json", "../package.json")).toBe(true)
    expect(linkStaysInside("a", "../x")).toBe(false)
    expect(linkStaysInside("a/l", "b/../../x")).toBe(false)
    expect(linkStaysInside("l", "/etc")).toBe(false)
    expect(linkStaysInside("l", "")).toBe(false)
  })
})
