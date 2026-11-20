import { useQuery } from "@tanstack/react-query"

import type { Schemas } from "@/api/client"
import { namespaceQuery } from "@/api/queries"

/** Logs are shown unless the access review says the person may not read them, as a Viewer. */
export const logsReadable = (namespace?: Pick<Schemas.ProjectNamespace, "canReadLogs">) =>
  namespace?.canReadLogs !== "denied"

export function useLogsReadable(project: string | undefined): boolean {
  const namespace = useQuery({ ...namespaceQuery(project ?? ""), enabled: Boolean(project) })
  return logsReadable(namespace.data)
}
