// The server's YAML leaves out bookkeeping fields and hides a Secret's values; the status comes
// beside it.
import { useQuery } from "@tanstack/react-query"
import { useState } from "react"
import GitBranchIcon from "~icons/ph/git-branch"
import XIcon from "~icons/ph/x"

import { objectQuery, type ObjectAddress } from "@/api/queries"
import { ObjectOverview } from "@/components/object-overview"
import { LoadingRows } from "@/components/page"
import { Problem } from "@/components/problem"
import { Badge } from "@zeabur/design-system/components/badge"
import { Button } from "@zeabur/design-system/components/button"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogTitle,
} from "@zeabur/design-system/components/dialog"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@zeabur/design-system/components/tabs"
import { YamlView } from "@/components/yaml-view"
import { useT } from "@/i18n"
import { kindName } from "@/i18n/kind-name"

export function ObjectSheet({
  project,
  address,
  onClose,
}: {
  project: string
  address: ObjectAddress
  onClose: () => void
}) {
  const t = useT()
  const object = useQuery(objectQuery(project, address, { poll: true }))
  const kind = object.data?.origin.kind
  const [tab, setTab] = useState<"overview" | "yaml">("overview")
  return (
    <Dialog open onOpenChange={(open) => (open ? undefined : onClose())}>
      <DialogContent variant="panel">
        <div className="flex items-center gap-2 border-b px-6 py-3">
          <DialogTitle className="flex min-w-0 flex-1 items-baseline">
            <span className="truncate font-mono" translate="no">
              {address.name}
            </span>
            <span
              className="ml-2 shrink-0 type-meta font-normal text-muted-foreground"
              translate="no"
            >
              {kind && kindName(t, kind) !== kind ? `${kindName(t, kind)} · ` : ""}
              {kind ?? address.resource}
              {address.group ? ` ${address.group}` : ""}
            </span>
          </DialogTitle>
          {object.data?.provenance.tool ? (
            <Badge variant="outline">
              <GitBranchIcon aria-hidden="true" />
              {t("common.managedBy", object.data.provenance.tool)}
            </Badge>
          ) : null}
          <DialogClose
            render={
              <Button
                variant="ghost"
                size="icon-sm"
                className="-mr-1"
                aria-label={t("common.close")}
              />
            }
          >
            <XIcon aria-hidden="true" />
          </DialogClose>
        </div>
        {object.isPending ? (
          <div className="px-6 py-5">
            <LoadingRows label={t("common.loading")} rows={8} />
          </div>
        ) : object.isError ? (
          <div className="px-6 py-5">
            <Problem error={object.error} />
          </div>
        ) : (
          <Tabs
            value={tab}
            onValueChange={(value) => setTab(value === "yaml" ? "yaml" : "overview")}
            variant="page"
            className="min-h-0 flex-1"
          >
            <div className="px-6 pt-4">
              <TabsList aria-label={t("object.tabs")}>
                <TabsTrigger value="overview">{t("tab.overview")}</TabsTrigger>
                <TabsTrigger value="yaml">{t("tab.yaml")}</TabsTrigger>
              </TabsList>
            </div>
            <div className="min-h-0 flex-1 overflow-y-auto px-6 py-5">
              <TabsContent value="overview">
                <ObjectOverview
                  project={project}
                  address={address}
                  kind={object.data.origin.kind}
                  status={object.data.status}
                />
              </TabsContent>
              <TabsContent value="yaml">
                <YamlView
                  project={project}
                  address={address}
                  yaml={object.data.yaml}
                  managedBy={object.data.provenance.tool}
                  kind={kind}
                  onDeleted={onClose}
                />
              </TabsContent>
            </div>
          </Tabs>
        )}
      </DialogContent>
    </Dialog>
  )
}
