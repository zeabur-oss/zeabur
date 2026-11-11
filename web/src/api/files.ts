// The files of a container (`/api/v1/projects/{project}/pods/{pod}/files`).
import { queryOptions } from "@tanstack/react-query"

import * as zeabur from "@/api/zeabur"
import { queryClient } from "@/api/queries"

/** Where the calls go: the pod, its container, and the debug container that reads for it. */
export interface Place {
  project: string
  pod: string
  container: string
  via?: string
}

const filesKey = (project: string, pod: string) => ["project", project, "files", pod]

export const filesQuery = (project: string, pod: string, container: string) =>
  queryOptions({
    queryKey: [...filesKey(project, pod), container],
    queryFn: async ({ signal }) => await zeabur.getFiles(project, pod, { container }, { signal }),
    staleTime: 0,
  })

export const listQuery = (p: Place, path: string, filter: string) =>
  queryOptions({
    queryKey: [...filesKey(p.project, p.pod), p.container, p.via, "list", path, filter],
    queryFn: async ({ signal }) =>
      await zeabur.listFiles(
        p.project,
        p.pod,
        { container: p.container, via: p.via, path, filter: filter || undefined },
        { signal },
      ),
    staleTime: 5_000,
  })

export const previewQuery = (p: Place, path: string) =>
  queryOptions({
    queryKey: [...filesKey(p.project, p.pod), p.container, p.via, "preview", path],
    queryFn: async ({ signal }) =>
      await zeabur.previewFile(
        p.project,
        p.pod,
        { container: p.container, via: p.via, path },
        { signal },
      ),
    staleTime: 0,
  })

/** Marks every read of a pod's files as out of date. */
export const refreshFiles = (project: string, pod: string) =>
  queryClient.invalidateQueries({ queryKey: filesKey(project, pod) })
