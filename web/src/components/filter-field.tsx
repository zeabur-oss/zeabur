import { useEffect, useRef } from "react"
import MagnifyingGlassIcon from "~icons/ph/magnifying-glass"

import { Kbd } from "@zeabur/design-system/components/kbd"
import { cn } from "@/lib/utils"

/** A filter above a long collection. With `slash`, `/` focuses it from anywhere on the page. */
export function FilterField({
  value,
  onChange,
  label,
  slash = false,
  wide = false,
}: {
  value: string
  onChange: (value: string) => void
  label: string
  slash?: boolean
  /** Fills its row instead of keeping to a short field. */
  wide?: boolean
}) {
  const ref = useRef<HTMLInputElement>(null)
  useEffect(() => {
    if (!slash) return
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "/" || event.metaKey || event.ctrlKey || event.altKey) return
      const target = event.target
      if (
        target instanceof HTMLElement &&
        (target.isContentEditable || target.closest("input, textarea, select, [role=dialog]"))
      )
        return
      event.preventDefault()
      ref.current?.focus()
    }
    document.addEventListener("keydown", onKey)
    return () => document.removeEventListener("keydown", onKey)
  }, [slash])
  return (
    <label
      className={cn(
        "flex h-8 w-full items-center gap-2 rounded-lg border bg-background px-2.5 focus-within:border-ring",
        !wide && "sm:w-64",
      )}
    >
      <MagnifyingGlassIcon className="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
      <input
        ref={ref}
        type="search"
        value={value}
        onChange={(event) => onChange(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Escape" && value) {
            event.stopPropagation()
            onChange("")
          }
        }}
        aria-label={label}
        aria-keyshortcuts={slash ? "/" : undefined}
        className="w-full min-w-0 bg-transparent text-sm outline-none"
      />
      {slash && !value ? <Kbd keys="/" aria-hidden="true" className="max-md:hidden" /> : null}
    </label>
  )
}
