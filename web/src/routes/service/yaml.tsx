// A service's workload as YAML: what Kubernetes holds, without the server's bookkeeping and with
// no Secret values (a workload names Secrets but never holds their values). Edit plans the change.
import { useQuery } from "@tanstack/react-query"

import { objectQuery } from "@/api/queries"
import { LoadingRows } from "@/components/page"
import { Problem } from "@/components/problem"
import { YamlView } from "@/components/yaml-view"
import { useT } from "@/i18n"
import { addressOf } from "@/lib/objects"
import type { Service } from "@/model/service"

export function YamlTab({ project, service }: { project: string; service: Service }) {
  const t = useT()
  const address = addressOf(service.kind, service.name)
  const object = useQuery({
    ...objectQuery(project, address ?? { group: "", version: "", resource: "", name: "" }),
    enabled: address !== undefined,
  })
  if (!address) return null
  if (object.isPending) return <LoadingRows label={t("common.loading")} rows={8} />
  if (object.isError) return <Problem error={object.error} />
  return (
    <YamlView
      project={project}
      address={address}
      yaml={object.data.yaml}
      managedBy={object.data.provenance.tool}
    />
  )
}
