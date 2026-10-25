// A service from an uploaded folder: Zeabur Builder's Build with an archive source, and the
// objects of an image service whose image is the Build's, as for Git (git-service.ts). The Build
// is created with a token request, which Zeabur Builder answers; the folder goes up after that
// (upload.ts).
import type { Schemas } from "@/api/client"
import { builtObjects, builtProblems, cleanPath, type BuiltService } from "@/model/git-service"
import type { DomainSettings } from "@/model/image-service"

type Change = Schemas["Change"]
type Manifest = Record<string, unknown>

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

/** The Build's `spec.source`: an uploaded folder, with no digest until it is pushed. */
function archiveSource(form: UploadService): Manifest {
  const directory = cleanPath(form.directory)
  const dockerfile = form.method === "dockerfile" ? cleanPath(form.dockerfile) : ""
  return {
    archive: {
      ...(directory ? { contextDir: directory } : {}),
      ...(dockerfile ? { dockerfile } : {}),
    },
  }
}

/** The annotation whose value Zeabur Builder answers with an upload token. */
export const UPLOAD_TOKEN_REQUEST = "builder.zeabur.com/upload-token-request"

/** The objects the form creates; the Build asks for its upload token as it is made. */
export function uploadObjects(
  form: UploadService,
  project: string,
  registry: string,
  domains: DomainSettings,
  request: string,
): Manifest[] {
  return builtObjects(form, project, registry, domains, {
    source: archiveSource(form),
    annotations: { [UPLOAD_TOKEN_REQUEST]: request },
  })
}

export function createUploadService(
  form: UploadService,
  project: string,
  registry: string,
  domains: DomainSettings,
  request: string,
): Change[] {
  return uploadObjects(form, project, registry, domains, request).map((object) => ({
    op: "create",
    object: object as Record<string, never>,
  }))
}
