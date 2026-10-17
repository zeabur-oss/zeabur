import { Link } from "@tanstack/react-router"
import MapTrifoldIcon from "~icons/ph/map-trifold"

import { buttonVariants } from "@/components/ui/button"
import { Empty, EmptyContent, EmptyHeader, EmptyMedia, EmptyTitle } from "@/components/ui/empty"
import { useT } from "@/i18n"
import { cn } from "@/lib/utils"

export function NotFound() {
  const t = useT()
  return (
    <Empty>
      <EmptyHeader>
        <EmptyMedia variant="icon">
          <MapTrifoldIcon aria-hidden="true" />
        </EmptyMedia>
        <EmptyTitle>{t("common.notFound")}</EmptyTitle>
      </EmptyHeader>
      <EmptyContent>
        <Link to="/cluster" className={cn(buttonVariants({ variant: "outline" }))}>
          {t("common.backToProjects")}
        </Link>
      </EmptyContent>
    </Empty>
  )
}
