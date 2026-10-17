// A route that failed to load: the problem, and a way to try again.
import type { ErrorComponentProps } from "@tanstack/react-router"

import { Problem } from "@/components/problem"
import { Button } from "@/components/ui/button"
import { useT } from "@/i18n"

export function RouteErrorView({ error, reset }: ErrorComponentProps) {
  const t = useT()
  return (
    <div className="flex flex-col items-start gap-3 px-4 py-6 md:px-8">
      <Problem error={error} retry={reset} />
      <Button variant="outline" size="sm" onClick={reset}>
        {t("common.retry")}
      </Button>
    </div>
  )
}
