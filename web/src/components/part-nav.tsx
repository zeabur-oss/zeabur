import type { ReactNode } from "react"

/** Links to the parts of a Settings page; each scrolls to its anchor and keeps it in the URL. */
export function PartNav<Part extends string>({
  label,
  parts,
  name,
  mark,
}: {
  label: string
  parts: readonly Part[]
  name: (part: Part) => string
  mark?: (part: Part) => ReactNode
}) {
  return (
    <nav aria-label={label} className="flex flex-wrap items-center gap-x-4 gap-y-1">
      {parts.map((part) => (
        <a
          key={part}
          href={`#${part}`}
          onClick={(event) => {
            event.preventDefault()
            document.getElementById(part)?.scrollIntoView({ block: "start", behavior: "smooth" })
            history.replaceState(history.state, "", `#${part}`)
          }}
          className="flex items-center gap-1.5 type-meta text-secondary outline-none hover:text-main focus-visible:underline"
        >
          {name(part)}
          {mark?.(part)}
        </a>
      ))}
    </nav>
  )
}
