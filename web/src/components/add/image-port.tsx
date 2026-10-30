import { useQuery } from "@tanstack/react-query"
import { useEffect, useState } from "react"

import type { Schemas } from "@/api/client"
import { imageInspectionQuery } from "@/api/queries"
import { useT } from "@/i18n"
import { imageProblem } from "@/model/image-service"
import type { PortNote } from "@/model/image-ports"
import { Button } from "@zeabur/design-system/components/button"

const SETTLE_MS = 600

/** The image's configuration, once the image (and its registry login) stops changing. */
export function useImageInspection(project: string, request: Schemas.InspectRequest) {
  const key = JSON.stringify(request)
  const [settled, setSettled] = useState(key)
  useEffect(() => {
    const timer = setTimeout(() => setSettled(key), SETTLE_MS)
    return () => clearTimeout(timer)
  }, [key])
  const body = JSON.parse(settled) as Schemas.InspectRequest
  return useQuery({
    ...imageInspectionQuery(project, body),
    enabled: imageProblem(body.image) === undefined,
  })
}

/** The port field's hint when the image says something about its port. */
export function PortNoteHint({
  note,
  port,
  onPick,
}: {
  note: PortNote
  port: string
  onPick: (port: string) => void
}) {
  const t = useT()
  if (note.kind === "read") return t("add.portRead", String(note.port))
  if (note.kind === "unreadable")
    return note.reason === "no-platform" && note.detail
      ? t("add.portNoPlatform", note.detail)
      : t(
          `add.portUnreadable.${note.reason === "not-found" || note.reason === "unauthorized" ? note.reason : "other"}`,
        )
  return (
    <span className="flex flex-wrap items-center gap-x-1 gap-y-1">
      <span>
        {note.kind === "choose"
          ? t("add.portChoose", note.ports.join(", "), port.trim())
          : t("add.portDeclared", note.ports.join(", "))}
      </span>
      {note.ports
        .filter((p) => String(p) !== port.trim())
        .map((p) => (
          <Button key={p} type="button" variant="ghost" size="xs" onClick={() => onPick(String(p))}>
            {t("add.portUse", String(p))}
          </Button>
        ))}
    </span>
  )
}
