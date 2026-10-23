import { useEffect, useRef } from "react"
import MagnifyingGlassIcon from "~icons/ph/magnifying-glass"

import { cn } from "@/lib/utils"

/** A filter field; with `slash`, `/` anywhere outside a field jumps to it. */
export function FilterBox({
  value,
  onChange,
  label,
  slash = false,
  className,
}: {
  value: string
  onChange: (value: string) => void
  label: string
  slash?: boolean
  className?: string
}) {
  const input = useRef<HTMLInputElement>(null)
  useEffect(() => {
    if (!slash) return
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "/" || event.metaKey || event.ctrlKey || event.altKey) return
      const target = event.target as HTMLElement | null
      if (target?.closest("input, textarea, select, [contenteditable=true], [role=dialog]")) return
      event.preventDefault()
      input.current?.focus()
    }
    window.addEventListener("keydown", onKey)
    return () => window.removeEventListener("keydown", onKey)
  }, [slash])
  return (
    <label
      className={cn(
        "flex h-8 w-full items-center gap-2 rounded-lg border bg-background px-2.5 focus-within:border-ring",
        className,
      )}
    >
      <MagnifyingGlassIcon className="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
      <input
        ref={input}
        type="search"
        value={value}
        onChange={(event) => onChange(event.target.value)}
        aria-label={label}
        placeholder={label}
        aria-keyshortcuts={slash ? "/" : undefined}
        className="w-full min-w-0 bg-transparent text-sm outline-none placeholder:text-muted-foreground"
      />
    </label>
  )
}
