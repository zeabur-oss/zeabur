import { expect, test } from "bun:test"

import { addEntries, type AddChoices } from "@/components/add/menu"
import { translator } from "@/i18n"
import { en } from "@/i18n/en"

const t = translator(en)
const open = { open: true } as const

const choices = (overrides: Partial<AddChoices>): AddChoices => ({
  image: true,
  databases: [],
  git: { open: true, gate: open, install: [], keelUnavailable: false, registrySetting: false },
  yaml: false,
  none: false,
  gates: { image: open, database: open },
  ...overrides,
})

test("an entry whose operator is missing opens that component's install", () => {
  const operator = { id: "cloudnative-pg", name: "CloudNativePG" }
  const entries = addEntries(
    choices({ databases: [{ engine: "postgresql", installed: false, operator }] }),
    t,
  )
  const postgres = entries.find((e) => e.what === "postgresql")
  expect(postgres?.why).toBe(t("add.installFirst", "CloudNativePG"))
  expect(postgres?.install).toEqual(operator)
})

test("Deploy from Git opens the first of the components it lacks", () => {
  const builder = { id: "zeabur-builder", name: "Zeabur Builder" }
  const keel = { id: "keel", name: "Keel" }
  const git = {
    open: false,
    gate: open,
    install: [builder, keel],
    keelUnavailable: false,
    registrySetting: false,
  }
  const entry = addEntries(choices({ git }), t).find((e) => e.what === "git")
  expect(entry?.install).toEqual(builder)
})
