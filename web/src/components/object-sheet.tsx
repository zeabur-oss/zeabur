// One object's YAML in a panel along the right edge: the Objects page, the project column's other
// objects and ⌘K open it. The server writes the YAML without its bookkeeping and with a Secret's
// values hidden; Edit plans a change to it, Delete its deletion.
import { useQuery } from "@tanstack/react-query"
import GitBranchIcon from "~icons/ph/git-branch"
import XIcon from "~icons/ph/x"

import { objectQuery, type ObjectAddress } from "@/api/queries"
import { LoadingRows } from "@/components/page"
import { Problem } from "@/components/problem"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Dialog, DialogClose, DialogContent, DialogTitle } from "@/components/ui/dialog"
import { YamlView } from "@/components/yaml-view"
import { useT } from "@/i18n"

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
  const object = useQuery(objectQuery(project, address))
  const kind = object.data?.origin.kind
  return (
    <Dialog open onOpenChange={(open) => (open ? undefined : onClose())}>
      <DialogContent variant="panel">
        <div className="flex items-center gap-2 border-b px-5 py-3">
          <DialogTitle className="flex min-w-0 flex-1 items-baseline">
            <span className="truncate font-mono" translate="no">
              {address.name}
            </span>
            <span
              className="ml-2 shrink-0 type-meta font-normal text-muted-foreground"
              translate="no"
            >
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
            render={<Button variant="ghost" size="icon-sm" aria-label={t("common.close")} />}
          >
            <XIcon aria-hidden="true" />
          </DialogClose>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto p-5">
          {object.isPending ? (
            <LoadingRows label={t("common.loading")} rows={8} />
          ) : object.isError ? (
            <Problem error={object.error} />
          ) : (
            <YamlView
              project={project}
              address={address}
              yaml={object.data.yaml}
              managedBy={object.data.provenance.tool}
              kind={kind}
              onDeleted={onClose}
            />
          )}
        </div>
      </DialogContent>
    </Dialog>
  )
}
