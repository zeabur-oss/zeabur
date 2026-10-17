import { test, expect } from "bun:test"

import type { Translate } from "@/i18n"
import { en } from "@/i18n/en"
import { k3sPackagedNotes } from "@/routes/cluster/k3s-packaged"

// A translator that writes a message as its key and arguments, so a copy edit changes no test.
const t = ((key: string, ...args: unknown[]) =>
  args.length > 0 ? `${key}(${args.join(", ")})` : key) as unknown as Translate

test("a component k3s packages gets one warning per add-on, with what disabling it removes", () => {
  expect(k3sPackagedNotes(["traefik"], t)).toStrictEqual([
    "install.k3sPackaged(traefik) install.k3sPackagedTraefik",
  ])
  expect(k3sPackagedNotes(["metrics-server"], t)).toStrictEqual([
    "install.k3sPackaged(metrics-server) install.k3sPackagedMetrics",
  ])
})

test("any other component gets no warning", () => {
  expect(k3sPackagedNotes([], t)).toStrictEqual([])
})

test("the English warning names the add-on and the k3s config file", () => {
  const english: Translate = (key, ...args) => {
    const message = en[key] as string | ((...a: unknown[]) => string)
    return typeof message === "function" ? message(...args) : message
  }
  const [note = ""] = k3sPackagedNotes(["traefik"], english)
  expect(note).toMatch(/packaged traefik/)
  expect(note).toMatch(/\/etc\/rancher\/k3s\/config\.yaml/)
})
