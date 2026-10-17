// What a route shows while its loader waits (after the router's pending delay) and when loading
// it fails outright, such as a page's code that could not be fetched. Answers from the cluster
// (forbidden, not found) are not route errors: each page shows those in place. The error view
// loads only when one happens, so it adds nothing to the first page load.
import type { ErrorComponentProps } from "@tanstack/react-router"
import { lazy, Suspense } from "react"

import { LoadingRows } from "@/components/page"
import { useT } from "@/i18n"

export function RoutePending() {
  const t = useT()
  return (
    <div className="px-4 py-6 md:px-8">
      <LoadingRows label={t("common.loading")} rows={6} />
    </div>
  )
}

const RouteErrorView = lazy(() =>
  import("@/components/route-error").then((m) => ({ default: m.RouteErrorView })),
)

export function RouteError(props: ErrorComponentProps) {
  return (
    <Suspense fallback={null}>
      <RouteErrorView {...props} />
    </Suspense>
  )
}
