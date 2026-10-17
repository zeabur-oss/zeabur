import { describe, test, expect } from "bun:test"

import { createElement as h, type ReactElement } from "react"
import { renderToStaticMarkup } from "react-dom/server"

import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog"
import { Menu, MenuContent, MenuItem, MenuTrigger } from "@/components/ui/menu"
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover"
import { loader, pendingTriggerProps, popups } from "@/components/ui/popup"
import { Tooltip } from "@/components/ui/tooltip"

/** Markup with each tag's attributes in order, and without what Base UI adds for its own
 * bookkeeping: generated ids, data-base-ui-* markers, and tabindex="0" on a native button (where
 * it changes nothing). */
const html = (element: ReactElement) =>
  renderToStaticMarkup(element)
    .replace(/ id="base-ui-[^"]*"/g, "")
    .replace(/ data-base-ui-[a-z-]+="[^"]*"/g, "")
    .replace(/ tabindex="0"/g, "")
    .replace(/<([a-z]+)((?: [a-z-]+="[^"]*")*)>/g, (_, tag: string, attributes: string) => {
      const sorted = (attributes.match(/ [a-z-]+="[^"]*"/g) ?? []).sort().join("")
      return `<${tag}${sorted}>`
    })

const menu = (props: { open?: boolean; onOpenChange?: (open: boolean) => void }) =>
  h(
    Menu,
    props,
    h(MenuTrigger, { render: h("button", { type: "button" }), "aria-label": "Account" }, "A"),
    h(MenuContent, null, h(MenuItem, null, "Sign out")),
  )
const popover = (props: { open?: boolean }) =>
  h(
    Popover,
    props,
    h(PopoverTrigger, { variant: "plain", "aria-label": "Language" }, "L"),
    h(PopoverContent, null, "English"),
  )
const tooltip = () =>
  h(Tooltip, { content: "Light", children: h("button", { type: "button" }, "T") })

/** The handlers a trigger has before the code arrives, from a render function. */
function pendingHandlers(trigger: (render: ReactElement) => ReactElement) {
  let handlers: Record<string, unknown> = {}
  const capture = ((props: Record<string, unknown>) => {
    handlers = props
    return h("span", props)
  }) as unknown as ReactElement
  renderToStaticMarkup(trigger(capture))
  return handlers as ReturnType<typeof pendingTriggerProps>
}

const key = (name: string, tagName: string) => {
  let prevented = false
  const event = {
    key: name,
    currentTarget: { tagName },
    preventDefault: () => (prevented = true),
  } as unknown as Parameters<ReturnType<typeof pendingTriggerProps>["onKeyDown"]>[0]
  return { event, prevented: () => prevented }
}

describe("popups before their code arrives", () => {
  // In order: the first tests run before the code is loaded, the last ones after.
  let before: Record<string, string>

  test("the code is not there yet", () => {
    expect(popups.get()).toBe(undefined)
    before = { menu: html(menu({})), popover: html(popover({})), tooltip: html(tooltip()) }
    expect(before.menu).toBe(
      '<button aria-expanded="false" aria-haspopup="menu" aria-label="Account" data-slot="menu-trigger" type="button">A</button>',
    )
    expect(before.popover!).toMatch(/^<button aria-expanded="false" aria-haspopup="dialog" /)
    expect(before.menu! + before.popover!).not.toMatch(/Sign out|English/)
  })

  test("a press asks to open, by click, or Enter or Space on an element that is no button", () => {
    const asked: boolean[] = []
    const trigger = (render: ReactElement) =>
      h(Menu, { onOpenChange: (open: boolean) => asked.push(open) }, h(MenuTrigger, { render }))
    const handlers = pendingHandlers(trigger)
    handlers.onClick()
    expect(asked).toStrictEqual([true])

    for (const name of ["Enter", " "]) {
      const span = key(name, "SPAN")
      handlers.onKeyDown(span.event)
      expect(span.prevented()).toBeTruthy()
    }
    expect(asked).toStrictEqual([true, true, true])
    // A native button turns Enter and Space into the click itself.
    const button = key("Enter", "BUTTON")
    handlers.onKeyDown(button.event)
    expect(!button.prevented()).toBeTruthy()
    // Other keys do nothing.
    handlers.onKeyDown(key("a", "SPAN").event)
    expect(asked).toStrictEqual([true, true, true])
  })

  test("an open popup asks for its code, and shows nothing until it is there", async () => {
    const markup = renderToStaticMarkup(
      h(Dialog, { open: true }, h(DialogContent, null, h(DialogTitle, null, "Find"))),
    )
    expect(markup).toBe("")
    // The render started the one load; waiting for it gives the code.
    const parts = await popups.load()
    expect(parts).toBeTruthy()
    expect(popups.get()).toBe(parts)
  })

  // Base UI's triggers take their open state from the browser (e2e/ui.spec.ts: "a popup pressed
  // before its code arrives"), so here only the closed markup is compared.
  test("once it is there, the triggers render the same", () => {
    expect(html(menu({}))).toBe(before.menu)
    expect(html(popover({}))).toBe(before.popover)
    expect(html(tooltip())).toBe(before.tooltip)
  })
})

describe("loading once", () => {
  test("concurrent loads share one, subscribers hear of it, and a failure is tried again", async () => {
    let calls = 0
    let fail = true
    const module = loader(async () => {
      calls++
      if (fail) throw new Error("offline")
      return "parts"
    })
    let heard = 0
    module.subscribe(() => heard++)
    expect(await module.load()).toBe(undefined)
    expect(module.get()).toBe(undefined)
    fail = false
    const [a, b] = await Promise.all([module.load(), module.load()])
    expect(a).toBe("parts")
    expect(b).toBe("parts")
    expect(module.get()).toBe("parts")
    expect(calls).toBe(2)
    expect(heard).toBe(1)
  })
})
