import { invoke } from "@tauri-apps/api/core"
import { Button } from "@zeabur-oss/design-system/components/button"
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@zeabur-oss/design-system/components/empty"
import { Illustration } from "@zeabur-oss/design-system/components/illustration"
import { Spinner } from "@zeabur-oss/design-system/components/spinner"
import { useQuery } from "@tanstack/react-query"

import { useI18n } from "@/i18n"

/** The page a cluster window shows until its local service answers (`?name=…`). The app knows
 * which context the window opens. The app loads the service's link in the window itself once it
 * answers, so the page only starts it and shows a failure. Starting is one query: React Query shares the call that React's
 * double effect in development makes, and a retry runs it again. Each start replaces the window's
 * service, so nothing starts it by itself again. */
export function ClusterPage() {
  const { t } = useI18n()
  const name = new URLSearchParams(window.location.search).get("name") ?? ""
  const { error, isFetching, refetch } = useQuery({
    queryKey: ["start"],
    queryFn: () => invoke<null>("start_cluster"),
    retry: false,
    staleTime: Infinity,
    refetchOnWindowFocus: false,
    refetchOnReconnect: false,
  })

  const title = <title>{`${name} - Zeabur`}</title>
  if (!error || isFetching) {
    return (
      <>
        {title}
        <Empty className="h-screen">
          <EmptyHeader>
            <EmptyMedia>
              <Spinner />
            </EmptyMedia>
            <EmptyTitle>{t("desktop.clusterOpening", name)}</EmptyTitle>
            <EmptyDescription>{t("desktop.clusterOpeningBody")}</EmptyDescription>
          </EmptyHeader>
        </Empty>
      </>
    )
  }

  return (
    <>
      {title}
      <Empty className="h-screen">
        <EmptyHeader>
          <EmptyMedia>
            <Illustration kind="error" />
          </EmptyMedia>
          <EmptyTitle>{t("desktop.clusterFailed", name)}</EmptyTitle>
          <EmptyDescription className="break-words select-text">{String(error)}</EmptyDescription>
        </EmptyHeader>
        <EmptyContent>
          <div className="flex flex-wrap justify-center gap-2">
            <Button onClick={() => void refetch()}>{t("desktop.clusterRetry")}</Button>
            <Button variant="secondary" onClick={() => void invoke("back_to_chooser")}>
              {t("desktop.clusterBack")}
            </Button>
          </div>
        </EmptyContent>
      </Empty>
    </>
  )
}
