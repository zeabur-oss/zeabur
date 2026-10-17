// Menus, popovers, tooltips and dialogs are Base UI's, and their code (with Floating UI's
// positioning) is the largest part of the interface the first page does not draw. It loads when the
// browser is next idle, or sooner when a trigger is pointed at, focused or pressed. Until then a
// trigger is the same element with the same ARIA state, and a press opens its popup as soon as the
// code is there. Every popup starts closed, so nothing else shows before it arrives.
import { mergeProps } from "@base-ui/react/merge-props"
import { useRender } from "@base-ui/react/use-render"
import {
  createContext,
  useContext,
  useState,
  useSyncExternalStore,
  type ComponentProps,
  type KeyboardEvent,
  type ReactElement,
  type ReactNode,
} from "react"

export type Parts = typeof import("./popup-parts")

/** A module loaded once, on first demand: `get` is undefined until it is there. */
export interface Loader<T> {
  get(): T | undefined
  /** Starts the load, or returns the one under way; a failed load is tried again next time. */
  load(): Promise<T | undefined>
  subscribe(listener: () => void): () => void
}

/** `arrived` runs `notify`, which tells the subscribers, when the module is there. */
export function loader<T>(
  load: () => Promise<T>,
  arrived: (notify: () => void) => void = (notify) => notify(),
): Loader<T> {
  let value: T | undefined
  let loading: Promise<T | undefined> | undefined
  const listeners = new Set<() => void>()
  return {
    get: () => value,
    load: () =>
      (loading ??= load().then(
        (module) => {
          value = module
          arrived(() => {
            for (const listener of listeners) listener()
          })
          return module
        },
        () => (loading = undefined),
      )),
    subscribe(listener) {
      listeners.add(listener)
      return () => {
        listeners.delete(listener)
      }
    },
  }
}

// The positions of an element's ancestors among their siblings, from <html> down.
const pathOf = (element: Element) => {
  const path: number[] = []
  for (let at = element; at.parentElement; at = at.parentElement)
    path.unshift(Array.prototype.indexOf.call(at.parentElement.children, at))
  return path
}

/** Telling the triggers replaces each with Base UI's, the same element in the same place: focus
 * moves to the new one if it was on the old one. */
function keepFocus(notify: () => void) {
  const focused = document.activeElement
  const path = focused && focused !== document.body ? pathOf(focused) : undefined
  notify()
  if (!path) return
  // After React has rendered the change, which it does in a microtask.
  setTimeout(() => {
    if (focused!.isConnected || document.activeElement !== document.body) return
    const element = path.reduce<Element | undefined>(
      (at, index) => at?.children[index],
      document.documentElement,
    )
    if (element instanceof HTMLElement) element.focus({ preventScroll: true })
  })
}

const inBrowser = typeof document !== "undefined"

export const popups = loader(() => import("./popup-parts"), inBrowser ? keepFocus : undefined)

if (inBrowser) {
  const load = () => void popups.load()
  if ("requestIdleCallback" in window) requestIdleCallback(load, { timeout: 2000 })
  else setTimeout(load, 1000)
}

/** Base UI's popups, once loaded. */
export function usePopups(): Parts | undefined {
  return useSyncExternalStore(popups.subscribe, popups.get, popups.get)
}

/** For the parts of a popup that render only while it is open, so only once the code is there. */
export function useLoadedPopups(): Parts {
  const parts = usePopups()
  if (!parts) throw new Error("a popup rendered before its code loaded")
  return parts
}

/** A popup's open state, held here (Base UI's root is always controlled) so that a press before
 * the code arrives opens it on arrival. An open popup without its code asks for it. */
export function useOpen(
  open: boolean | undefined,
  defaultOpen: boolean | undefined,
  onOpenChange: ((open: boolean) => void) | undefined,
) {
  const parts = usePopups()
  const [own, setOwn] = useState(defaultOpen ?? false)
  const current = open ?? own
  // Starting the one load is idempotent, so it may happen during render.
  if (current && !parts) void popups.load()
  const setOpen = (next: boolean) => {
    setOwn(next)
    onOpenChange?.(next)
  }
  return { parts, open: current, setOpen }
}

/** A popup root's props as this interface uses them. */
export type RootProps<Props> = Omit<Props, "onOpenChange" | "children"> & {
  onOpenChange?: (open: boolean) => void
  children?: ReactNode
}

/** A trigger's props as this interface uses them: a class name and an element to render. */
export type TriggerProps<Props> = Omit<Props, "className" | "style" | "render"> & {
  className?: string
  render?: ReactElement
}

const RequestOpen = createContext<() => void>(() => {})

/** The root of a popup whose code has not arrived: its trigger's press opens it. */
export function PendingRoot({
  setOpen,
  children,
}: {
  setOpen: (open: boolean) => void
  children?: ReactNode
}) {
  return <RequestOpen value={() => setOpen(true)}>{children}</RequestOpen>
}

/** The handlers of a trigger before the code is there: load at the first sign of use, and open
 * on a press. A native button turns Enter and Space into a click; another element is given them. */
export function pendingTriggerProps(open: () => void) {
  const preload = () => void popups.load()
  return {
    onPointerEnter: preload,
    onPointerDown: preload,
    onFocus: preload,
    onClick: open,
    onKeyDown: (event: KeyboardEvent<Element>) => {
      if (event.currentTarget.tagName === "BUTTON") return
      if (event.key !== "Enter" && event.key !== " ") return
      event.preventDefault()
      open()
    },
  }
}

/** A trigger while the code loads: the element Base UI's trigger renders, with its ARIA state. */
export function PendingTrigger({
  haspopup,
  render,
  ...props
}: TriggerProps<ComponentProps<"button">> & { haspopup: "menu" | "dialog" }) {
  const requestOpen = useContext(RequestOpen)
  return useRender({
    defaultTagName: "button",
    render,
    props: mergeProps<"button">(
      {
        type: "button",
        "aria-haspopup": haspopup,
        "aria-expanded": false,
        ...pendingTriggerProps(requestOpen),
      },
      props,
    ),
  })
}

/** A tooltip's trigger while the code loads: `children` as it is, telling whether it is pointed
 * at or focused, so the tooltip shows on arrival. */
export function PendingTooltipTrigger({
  children,
  onShow,
}: {
  children: ReactElement
  onShow: (show: boolean) => void
}) {
  const preload = () => void popups.load()
  return useRender({
    render: children,
    props: {
      onPointerEnter: () => {
        onShow(true)
        preload()
      },
      onPointerLeave: () => onShow(false),
      onFocus: () => {
        onShow(true)
        preload()
      },
      onBlur: () => onShow(false),
    },
  })
}
