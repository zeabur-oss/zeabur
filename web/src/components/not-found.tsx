import { Link } from "@tanstack/react-router"
import { buttonVariants } from "@zeabur-oss/design-system/components/button"
import {
  Empty,
  EmptyContent,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@zeabur-oss/design-system/components/empty"
import { useT } from "@/i18n"
import { cn } from "@/lib/utils"
import { Illustration } from "@zeabur-oss/design-system/components/illustration"

export function NotFound() {
  const t = useT()
  return (
    <Empty>
      <EmptyHeader>
        <EmptyMedia>
          <Illustration kind="not-found" className="w-48" />
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
