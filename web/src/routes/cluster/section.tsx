// A section of the cluster page: a heading with its count and an action at the right, then the
// section's own content, loading, empty or permission state.
import type { ReactNode, Ref } from "react"

export function Section({
  id,
  title,
  count,
  action,
  children,
  ref,
}: {
  id: string
  title: ReactNode
  count?: ReactNode
  action?: ReactNode
  children: ReactNode
  ref?: Ref<HTMLElement>
}) {
  return (
    <section ref={ref} id={id} aria-labelledby={`${id}-title`} className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-x-2.5 gap-y-2">
        <h2 id={`${id}-title`} className="type-heading">
          {title}
        </h2>
        {count !== undefined ? (
          <span className="type-meta text-muted-foreground">{count}</span>
        ) : null}
        {action ? <div className="ml-auto flex items-center">{action}</div> : null}
      </div>
      {children}
    </section>
  )
}
