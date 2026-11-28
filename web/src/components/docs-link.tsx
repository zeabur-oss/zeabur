import type { ReactNode } from "react"
import ArrowSquareOutIcon from "~icons/ph/arrow-square-out"

import { useI18n } from "@/i18n"
import { docsUrl } from "@/lib/docs"

export function DocsLink({
  page,
  anchor,
  children,
}: {
  page: string
  anchor?: string
  children: ReactNode
}) {
  const { locale } = useI18n()
  return (
    <a
      href={docsUrl(locale, page, anchor)}
      target="_blank"
      rel="noreferrer noopener"
      className="inline-flex items-center gap-1 self-start type-meta text-muted-foreground hover:text-foreground"
    >
      {children}
      <ArrowSquareOutIcon className="size-3" aria-hidden="true" />
    </a>
  )
}
