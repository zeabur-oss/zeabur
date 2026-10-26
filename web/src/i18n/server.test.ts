import { describe, test, expect } from "bun:test"
import { readdirSync, readFileSync } from "node:fs"
import { join, resolve } from "node:path"

import { en, type Messages } from "@/i18n/en"
import { zhCN } from "@/i18n/zh-CN"
import { zhTW } from "@/i18n/zh-TW"

const crates = resolve(import.meta.dir, "../../../crates")
const scopes = /^(diagnosis|gap|trace|incident)\./

function files(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) =>
    entry.isDirectory() ? files(join(dir, entry.name)) : [join(dir, entry.name)],
  )
}

/** The keys the Rust code builds with a literal, and the keys the fixtures hold. */
function sentKeys() {
  const found = new Set<string>()
  const read = (dir: string, pattern: RegExp) => {
    for (const file of files(dir))
      for (const match of readFileSync(file, "utf8").matchAll(pattern))
        if (scopes.test(match[1]!)) found.add(match[1]!)
  }
  read(join(crates, "zeabur-modules/src"), /Message::new\("([^"]+)"/g)
  for (const crate of ["zeabur-modules", "zeabur-gaps"])
    read(join(crates, crate, "fixtures"), /"?\bkey"?: *"?([\w.]+)/g)
  return [...found].sort()
}

const dictionaries = { en, "zh-TW": zhTW, "zh-CN": zhCN } as Record<string, Messages>
const say = (language: string, key: string, params: Record<string, string>) =>
  (dictionaries[language]![`sentence.${key}` as keyof Messages] as (p: typeof params) => string)(
    params,
  )

describe("server sentences", () => {
  test("every key the Rust code and fixtures send has words in every language", () => {
    const keys = sentKeys()
    expect(keys).not.toEqual([])
    const missing = keys.flatMap((key) =>
      Object.entries(dictionaries)
        .filter(([, messages]) => !(`sentence.${key}` in messages))
        .map(([language]) => `${language} ${key}`),
    )
    expect(missing).toEqual([])
  })

  test("a select picks the case for the value, and none when the value is absent", () => {
    const key = "diagnosis.runFailed.why.gaveUp"
    expect(say("en", key, { exitCode: "1" })).toBe(
      "The task gave up after its retries; the last run ended with exit code 1.",
    )
    expect(say("en", key, { backoffLimit: "0", exitCode: "1" })).toBe(
      "The task doesn't retry a failed run; this one ended with exit code 1.",
    )
    expect(say("zh-TW", key, { backoffLimit: "0", exitCode: "1" })).toBe(
      "這項工作失敗後不會重試，這次的結束碼是 1。",
    )
  })

  test("a duration is in minutes when it is whole minutes, else in seconds", () => {
    const key = "diagnosis.probeFailure.why.liveness"
    const params = { periodDefault: "false", thresholdDefault: "false", threshold: "2" }
    expect(say("en", key, { ...params, period: "60", total: "120" })).toBe(
      "The cluster checks every 1 minute and restarts the container after 2 failures in a row (about 2 minutes).",
    )
    expect(say("en", key, { ...params, period: "90", total: "180" })).toContain("every 90 seconds")
    expect(say("zh-TW", key, { ...params, period: "60", total: "120" })).toBe(
      "叢集每 1 分鐘檢查一次，連續失敗 2 次（約 2 分鐘）就會重新啟動容器。",
    )
  })

  test("a list joins its items with the language's separator", () => {
    const key = "gap.noNetworkPolicy"
    expect(say("en", key, { services: "api,web,worker" })).toContain(
      "protects api, web, worker: any replica in the cluster can reach them.",
    )
    expect(say("zh-TW", key, { services: "api,web" })).toContain("保護 api、web：")
    expect(say("zh-CN", key, { services: "api" })).toContain("保护 api：")
  })
})
