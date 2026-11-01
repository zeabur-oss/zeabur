import { useLayoutEffect, useRef, useState, type ReactNode } from "react"
import CaretRightIcon from "~icons/ph/caret-right"

import { useT } from "@/i18n"
import { cn } from "@/lib/utils"
import { Button } from "@zeabur-oss/design-system/components/button"

/** The tabs, with a button to scroll on while some of them are out of view. */
export function TabRow({ children }: { children: ReactNode }) {
  const t = useT()
  const ref = useRef<HTMLDivElement>(null)
  const [overflow, setOverflow] = useState<"none" | "more" | "end">("none")
  const list = () => ref.current?.querySelector<HTMLElement>("[role=tablist]") ?? undefined
  const showing = overflow !== "none"
  useLayoutEffect(() => {
    const row = list()
    if (!row) return
    const check = () =>
      setOverflow(
        row.scrollWidth <= row.clientWidth + 1
          ? "none"
          : row.scrollLeft + row.clientWidth >= row.scrollWidth - 1
            ? "end"
            : "more",
      )
    check()
    row.addEventListener("scroll", check, { passive: true })
    const observer = new ResizeObserver(check)
    observer.observe(row)
    return () => {
      row.removeEventListener("scroll", check)
      observer.disconnect()
    }
  }, [])
  // The button narrows the row: keep the chosen tab clear of it.
  useLayoutEffect(() => {
    const row = list()
    const active = row?.querySelector<HTMLElement>("[aria-selected=true]")
    if (!row || !active || !showing) return
    const end = active.offsetLeft + active.offsetWidth - row.scrollLeft
    if (end > row.clientWidth) row.scrollLeft += end - row.clientWidth + 24
  }, [showing])
  return (
    <div ref={ref} className="flex items-center gap-1 pt-1 pb-3">
      <div className="min-w-0 flex-1">{children}</div>
      {overflow === "none" ? null : (
        <Button
          variant="muted"
          size="icon-sm"
          aria-label={t("service.moreTabs")}
          className={cn(overflow === "end" && "invisible")}
          onClick={() => {
            const row = list()
            row?.scrollBy({
              left: row.clientWidth * 0.6,
              behavior: matchMedia("(prefers-reduced-motion: reduce)").matches ? "auto" : "smooth",
            })
          }}
        >
          <CaretRightIcon aria-hidden="true" />
        </Button>
      )}
    </div>
  )
}
