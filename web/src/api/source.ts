// Sends a packed folder to the Build it is for. XMLHttpRequest, not fetch, for the upload's
// progress.
import { ApiError, localProblem, type Problem, type Schemas } from "@/api/client"
import { REQUEST_HEADERS } from "@/auth/session"

/** The folder as raw bytes; the browser sets `Content-Length`. */
export function sendSource(
  project: string,
  build: string,
  request: string,
  blob: Blob,
  onProgress: (sent: number) => void,
) {
  return new Promise<Schemas["UploadedSource"]>((resolve, reject) => {
    const http = new XMLHttpRequest()
    http.open(
      "POST",
      `/api/v1/projects/${encodeURIComponent(project)}/builds/${encodeURIComponent(build)}/source?request=${encodeURIComponent(request)}`,
    )
    http.setRequestHeader("content-type", "application/gzip")
    for (const [name, value] of Object.entries(REQUEST_HEADERS)) http.setRequestHeader(name, value)
    http.upload.onprogress = (event) => onProgress(event.loaded)
    http.onload = () => {
      if (http.status >= 200 && http.status < 300)
        return resolve(JSON.parse(http.responseText) as Schemas["UploadedSource"])
      let problem: Problem
      try {
        problem = JSON.parse(http.responseText) as Problem
      } catch {
        problem = localProblem(http.status, `The server answered ${http.status}.`)
      }
      reject(new ApiError(http.status, problem))
    }
    http.onerror = () => reject(new ApiError(0, localProblem(0, "The upload was cut off.")))
    http.send(blob)
  })
}
