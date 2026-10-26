// A service from an uploaded folder: one change, which the server turns into Zeabur Builder's
// Build with an archive source and the objects of the service that runs its image. The Build is
// created with a token request, which Zeabur Builder answers; the folder goes up after that
// (upload.ts).
import type { Schemas } from "@/api/client"
import { builtProblems, cleanPath, type BuiltService } from "@/model/git-service"

type Change = Schemas["Change"]

/** `directory` is the folder inside the upload to build (empty: the whole upload). */
export type UploadService = BuiltService

export const newUploadService = (): UploadService => ({
  directory: "",
  method: "railpack",
  dockerfile: "",
  buildVariables: [],
  name: "",
  port: "",
  publicUrl: false,
  host: "",
  variables: [],
  size: "small",
  security: "restricted",
})

/** What is wrong with each field of a form whose name, port and host are already resolved. */
export function uploadProblems(
  form: UploadService,
  {
    taken,
    badEnv,
    badBuildEnv,
    hasFolder,
  }: { taken: readonly string[]; badEnv: boolean; badBuildEnv: boolean; hasFolder: boolean },
) {
  return { ...builtProblems(form, { taken, badEnv, badBuildEnv }), folder: !hasFolder }
}

/** The change that makes the service; the server builds the Build and the objects that run it. */
export function createUploadService(form: UploadService, request: string): Change {
  const port = form.port.trim()
  const directory = cleanPath(form.directory)
  const dockerfile = form.method === "dockerfile" ? cleanPath(form.dockerfile) : ""
  return {
    op: "createUploadService",
    name: form.name,
    request,
    method: form.method,
    ...(directory ? { directory } : {}),
    ...(dockerfile ? { dockerfile } : {}),
    ...(port ? { port: Number(port) } : {}),
    ...(port && form.publicUrl ? { host: form.host.trim() } : {}),
    variables: form.variables.flatMap((v) =>
      "value" in v ? [{ name: v.name, value: v.value }] : [],
    ),
    buildVariables: form.buildVariables,
    size: form.size,
    security: form.security,
  }
}
