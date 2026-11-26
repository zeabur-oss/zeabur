import { describe, expect, test } from "bun:test"

import {
  groupItems,
  pageActions,
  shownItems,
  type GroupNames,
  type Item,
} from "@/components/shell/palette-items"
import { translator } from "@/i18n"
import { en } from "@/i18n/en"

const t = translator(en)
const groups: GroupNames = {
  recent: "Recent",
  projects: "Projects",
  services: "Services",
  pods: "Pods",
  domains: "Domains",
  objects: "Objects",
  actions: "Actions",
}

describe("the palette's pages for administrators", () => {
  test("offers Components to a member the server lets open it", () => {
    const items = pageActions(t, groups, { components: true })
    expect(items.map((i) => i.href)).toEqual(["/cluster/components"])
    expect(items[0]?.group).toBe("Actions")
  })

  test("leaves Components out when the server does not say allowed", () => {
    expect(pageActions(t, groups, { components: false })).toEqual([])
    expect(pageActions(t, groups, {})).toEqual([])
  })
})

describe("what the palette shows", () => {
  const items: Item[] = [
    { id: "p", group: "Projects", label: "shop" },
    { id: "pod", group: "Pods", label: "shop-1" },
    { id: "a", group: "Actions", label: "Components", keywords: "puzzle" },
  ]

  test("without a query, leaves out the groups that only a query reaches", () => {
    expect(shownItems(items, "  ", groups).map((i) => i.id)).toEqual(["p", "a"])
  })

  test("with a query, keeps the items that hold every word, keywords included", () => {
    expect(shownItems(items, "shop 1", groups).map((i) => i.id)).toEqual(["pod"])
    expect(shownItems(items, "PUZZLE", groups).map((i) => i.id)).toEqual(["a"])
  })

  test("groups the items in the order they first appear", () => {
    const shown = groupItems([items[0]!, items[2]!, { id: "q", group: "Projects", label: "blog" }])
    expect(shown.map((g) => [g.label, g.items.length])).toEqual([
      ["Projects", 2],
      ["Actions", 1],
    ])
  })
})
