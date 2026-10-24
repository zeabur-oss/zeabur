import type { ReactNode } from "react"

import { PageHeader } from "@zeabur/design-system/components/page-header"

/** A step's title, what it is for, its sections, and the buttons that move between steps. */
export function StepPage({
  title,
  description,
  badges,
  children,
  back,
  next,
}: {
  title: ReactNode
  description: ReactNode
  badges?: ReactNode
  children: ReactNode
  back?: ReactNode
  next?: ReactNode
}) {
  return (
    <>
      <PageHeader title={title} badges={badges} description={description} />
      {children}
      {back || next ? (
        <div className="flex flex-wrap items-center gap-2 border-t border-v2-border pt-5">
          {back}
          <div className="ml-auto flex flex-wrap items-center gap-2">{next}</div>
        </div>
      ) : null}
    </>
  )
}
