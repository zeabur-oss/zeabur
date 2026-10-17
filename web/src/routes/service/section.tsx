// A titled section of a service tab: a plain title ("Public") and an action. The Kubernetes kind
// shows in the section's rows (the Via column, a tooltip), not after the title.
import type { ReactNode } from "react"

export function Section({
  title,
  action,
  children,
}: {
  title: string
  action?: ReactNode
  children: ReactNode
}) {
  return (
    <section className="flex min-w-0 flex-col gap-2">
      <div className="flex items-center gap-2">
        <h2 className="text-sm font-medium">{title}</h2>
        <span className="ml-auto">{action}</span>
      </div>
      {children}
    </section>
  )
}

/** A disabled edit control says why: the workload is managed elsewhere. */
