import { Link } from "@tanstack/react-router"

import { buttonVariants } from "@zeabur/design-system/components/button"
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@zeabur/design-system/components/empty"
import { Illustration } from "@zeabur/design-system/components/illustration"
import { useT } from "@/i18n"

/** A project with no services: what a service is, how one appears, and where the rest is. */
export function NoServices({ project, framed = true }: { project: string; framed?: boolean }) {
  const t = useT()
  return (
    <Empty variant={framed ? "outline" : "default"} size="sm">
      <EmptyHeader>
        <EmptyMedia>
          <Illustration kind="empty" seed="services" />
        </EmptyMedia>
        <EmptyTitle>{t("project.noServices")}</EmptyTitle>
        <EmptyDescription>{t("project.noServicesBody")}</EmptyDescription>
      </EmptyHeader>
      <EmptyContent>
        <Link
          to="/projects/$project/objects"
          params={{ project }}
          className={buttonVariants({ variant: "outline", size: "sm" })}
        >
          {t("project.seeObjects")}
        </Link>
      </EmptyContent>
    </Empty>
  )
}
