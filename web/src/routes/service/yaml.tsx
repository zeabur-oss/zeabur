// A service's workload as YAML: what Kubernetes holds, without the server's bookkeeping and with
// no Secret values (a workload names Secrets but never holds their values). Edit plans the change.
import { ObjectYaml } from "@/components/store-detail"
import { addressOf } from "@/lib/objects"
import type { Service } from "@/model/service"

export function YamlTab({ project, service }: { project: string; service: Service }) {
  const address = addressOf(service.kind, service.name)
  if (!address) return null
  return <ObjectYaml project={project} address={address} />
}
