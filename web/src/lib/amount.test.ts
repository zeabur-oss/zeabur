import { describe, expect, test } from "bun:test"

import { exactBytes, isAmount, kubeAmount, parseAmount, toKube } from "@/lib/amount"

describe("CPU", () => {
  test("reads Kubernetes millicores and plain cores alike", () => {
    expect(parseAmount("100m", "cpu")).toBe(0.1)
    expect(parseAmount("0.1", "cpu")).toBe(0.1)
    expect(parseAmount("0.1 cores", "cpu")).toBe(0.1)
    expect(parseAmount("1 core", "cpu")).toBe(1)
    expect(parseAmount("2 核心", "cpu")).toBe(2)
    expect(parseAmount("0.5核", "cpu")).toBe(0.5)
  })

  test("writes the Kubernetes form back", () => {
    expect(toKube("0.1 cores", "cpu")).toBe("100m")
    expect(toKube("0.25", "cpu")).toBe("250m")
    expect(toKube("2 核心", "cpu")).toBe("2")
    expect(toKube("100m", "cpu")).toBe("100m")
  })

  test("leaves text that is no amount as it is", () => {
    expect(toKube(" lots ", "cpu")).toBe("lots")
    expect(isAmount("lots", "cpu")).toBe(false)
    expect(isAmount("", "cpu")).toBe(true)
  })
})

describe("memory and storage", () => {
  test("reads Kubernetes suffixes and plain units alike", () => {
    expect(parseAmount("128Mi", "bytes")).toBe(128 * 2 ** 20)
    expect(parseAmount("128 MiB", "bytes")).toBe(128 * 2 ** 20)
    expect(parseAmount("1.5GiB", "bytes")).toBe(1.5 * 2 ** 30)
    expect(parseAmount("1k", "bytes")).toBe(1000)
  })

  test("writes the largest unit that divides the amount", () => {
    expect(toKube("5 GiB", "bytes")).toBe("5Gi")
    expect(toKube("1.5 GiB", "bytes")).toBe("1536Mi")
    expect(toKube("1500 MiB", "bytes")).toBe("1500Mi")
    expect(kubeAmount(0, "bytes")).toBe("0")
  })

  test("shows an amount exactly in a binary unit", () => {
    expect(exactBytes(1500 * 2 ** 20)).toEqual(["1500", "MiB"])
    expect(exactBytes(5 * 2 ** 30)).toEqual(["5", "GiB"])
    expect(exactBytes(100)).toEqual(["100", "B"])
  })
})
