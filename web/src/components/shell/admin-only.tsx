import type { ReactNode } from "react"
import { Link } from "@tanstack/react-router"
import { buttonVariants } from "@zeabur-oss/design-system/components/button"
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@zeabur-oss/design-system/components/empty"
import { Illustration } from "@zeabur-oss/design-system/components/illustration"

import { usePages } from "@/components/gate"
import { useT } from "@/i18n"
import { cn } from "@/lib/utils"
import { pagesOfRoute } from "@/model/pages"

/** The page at `route`, or the note when the server does not let the person open it. Nothing shows
 * until the server has answered, and a failed answer counts as no. */
export function GatedPage({ route, children }: { route: string; children: ReactNode }) {
  const pages = pagesOfRoute(route)
  const access = usePages(pages ?? [])
  if (pages === undefined || access.open) return children
  return access.pending ? null : <AdminOnly />
}

/** What a member sees in place of a page that is only for cluster administrators. */
function AdminOnly() {
  const t = useT()
  return (
    <Empty>
      <EmptyHeader>
        <EmptyMedia>
          <Illustration kind="forbidden" />
        </EmptyMedia>
        <EmptyTitle>{t("shell.adminOnly")}</EmptyTitle>
        <EmptyDescription>{t("shell.adminOnlyHint")}</EmptyDescription>
      </EmptyHeader>
      <EmptyContent>
        <Link to="/projects" className={cn(buttonVariants({ variant: "outline" }))}>
          {t("common.backToProjects")}
        </Link>
      </EmptyContent>
    </Empty>
  )
}
