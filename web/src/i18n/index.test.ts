import { describe, test, expect } from "bun:test"

import { catalogueLoader, translator, writeSentence } from "@/i18n"
import { en, type Messages } from "@/i18n/en"

const taiwanese = { ...en, "shell.home": "首頁" } as Messages
const mainland = { ...en, "shell.home": "首页" } as Messages

describe("catalogues", () => {
  test("English is there without loading anything", () => {
    const catalogues = catalogueLoader({
      "zh-TW": () => Promise.reject(new Error("not asked for")),
      "zh-CN": () => Promise.reject(new Error("not asked for")),
    })
    expect(catalogues.cached("en")).toStrictEqual({ locale: "en", messages: en })
    expect(catalogues.cached("zh-TW")).toBe(undefined)
  })

  test("switching loads a catalogue once and back to English needs no load", async () => {
    let loads = 0
    const catalogues = catalogueLoader({
      "zh-TW": () => {
        loads += 1
        return Promise.resolve(taiwanese)
      },
      "zh-CN": () => Promise.resolve(mainland),
    })
    expect(await catalogues.load("zh-TW")).toStrictEqual({ locale: "zh-TW", messages: taiwanese })
    expect(await catalogues.load("zh-CN")).toStrictEqual({ locale: "zh-CN", messages: mainland })
    expect(await catalogues.load("en")).toStrictEqual({ locale: "en", messages: en })
    expect(await catalogues.load("zh-TW")).toStrictEqual({ locale: "zh-TW", messages: taiwanese })
    expect(catalogues.cached("zh-TW")).toStrictEqual({ locale: "zh-TW", messages: taiwanese })
    expect(loads).toBe(1)
  })

  test("a chunk that fails to load gives English, and the next choice tries again", async () => {
    let fail = true
    const catalogues = catalogueLoader({
      "zh-TW": () =>
        fail
          ? Promise.reject(new TypeError("Failed to fetch dynamically imported module"))
          : Promise.resolve(taiwanese),
      "zh-CN": () => Promise.resolve(mainland),
    })
    expect(await catalogues.load("zh-TW")).toStrictEqual({ locale: "en", messages: en })
    expect(catalogues.cached("zh-TW")).toBe(undefined)
    fail = false
    expect(await catalogues.load("zh-TW")).toStrictEqual({ locale: "zh-TW", messages: taiwanese })
  })

  test("only the latest choice applies when an earlier one loads after it", async () => {
    let release: (messages: Messages) => void = () => {}
    const catalogues = catalogueLoader({
      "zh-TW": () => new Promise((resolve) => (release = resolve)),
      "zh-CN": () => Promise.resolve(mainland),
    })
    const slow = catalogues.choose("zh-TW")
    const fast = catalogues.choose("zh-CN")
    expect(await fast).toStrictEqual({ locale: "zh-CN", messages: mainland })
    release(taiwanese)
    expect(await slow).toBe(undefined)
  })
})

describe("server sentences", () => {
  test("a key this build does not know is not written", () => {
    expect(writeSentence(translator(en), { key: "diagnosis.fromTheFuture" })).toBe(undefined)
    expect(writeSentence(translator(en), { key: "shell.home" })).toBe(undefined)
  })

  test("a probe's why names its timing, says by default, and falls back without it", () => {
    const why = (probe: string, params?: Record<string, string>) =>
      writeSentence(translator(en), { key: `diagnosis.probeFailure.why.${probe}`, params })
    const defaults = {
      period: "10",
      periodDefault: "true",
      threshold: "3",
      thresholdDefault: "true",
    }
    expect(why("liveness", defaults)).toBe(
      "By default, the cluster checks every 10 seconds and restarts the container after 3 failures in a row (about 30 seconds).",
    )
    expect(why("startup", { ...defaults, threshold: "30", thresholdDefault: "false" })).toBe(
      "The container has 30 × 10 seconds (5 minutes) to pass the startup check; then the cluster restarts it. Every 10 seconds is the default.",
    )
    expect(why("readiness", { container: "api" })).toBe(
      "While the traffic check fails, the container gets no traffic from its private addresses.",
    )
  })
})
