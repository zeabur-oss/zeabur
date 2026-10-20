// A route that failed to load: the problem, and a way to try again.
import type { ErrorComponentProps } from "@tanstack/react-router"

import { Problem } from "@/components/problem"
import { Button } from "@zeabur/design-system/components/button"
import { useT } from "@/i18n"
import { Illustration } from "@zeabur/design-system/components/illustration"

export function RouteErrorView({ error, reset }: ErrorComponentProps) {
  const t = useT()
  return (
    <div className="flex flex-col items-start gap-3 px-4 py-6 md:px-8">
      <Illustration kind="error" className="mb-2" />
      <Problem error={error} retry={reset} />
      <Button variant="outline" size="sm" onClick={reset}>
        {t("common.retry")}
      </Button>
    </div>
  )
}
