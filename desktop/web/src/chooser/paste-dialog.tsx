import { Button } from "@zeabur-oss/design-system/components/button"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogTitle,
} from "@zeabur-oss/design-system/components/dialog"
import { Input } from "@zeabur-oss/design-system/components/input"
import { Label } from "@zeabur-oss/design-system/components/label"
import { Textarea } from "@zeabur-oss/design-system/components/textarea"
import { useRef } from "react"
import XIcon from "~icons/ph/x"

import { useI18n, type Translate } from "@/i18n"

import { useChanges } from "./hooks"

export interface Paste {
  name: string
  yaml: string
  error: string | null
}

function pasteErrorText(error: unknown, t: Translate): string {
  const reason = String(error)
  return reason === "no-contexts" ? t("desktop.pasteEmpty") : t("desktop.pasteError", reason)
}

interface Props {
  paste: Paste | null
  setPaste: (paste: Paste | null) => void
}

export function PasteDialog({ paste, setPaste }: Props) {
  const { t } = useI18n()
  const { addPasted } = useChanges()
  const nameRef = useRef<HTMLInputElement>(null)
  if (!paste) return null

  function submit(current: Paste) {
    addPasted.mutate(current, {
      onSuccess: () => setPaste(null),
      onError: (error) => setPaste({ ...current, error: pasteErrorText(error, t) }),
    })
  }

  return (
    <Dialog open onOpenChange={(next) => (next ? undefined : setPaste(null))}>
      <DialogContent initialFocus={nameRef}>
        <div className="flex items-center gap-2 border-b px-5 py-3">
          <div className="min-w-0 flex-1 truncate">
            <DialogTitle>{t("desktop.pasteTitle")}</DialogTitle>
          </div>
          <DialogClose
            render={<Button variant="ghost" size="icon-sm" aria-label={t("desktop.cancel")} />}
          >
            <XIcon aria-hidden="true" />
          </DialogClose>
        </div>
        <form
          className="flex flex-col gap-4 px-5 py-4"
          onSubmit={(event) => {
            event.preventDefault()
            submit(paste)
          }}
        >
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="paste-name">{t("desktop.pasteName")}</Label>
            <Input
              id="paste-name"
              ref={nameRef}
              value={paste.name}
              onChange={(e) => setPaste({ ...paste, name: e.target.value })}
            />
          </div>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="paste-yaml">{t("desktop.pasteYaml")}</Label>
            <Textarea
              id="paste-yaml"
              rows={12}
              spellCheck={false}
              mono
              value={paste.yaml}
              onChange={(e) => setPaste({ ...paste, yaml: e.target.value, error: null })}
            />
            {paste.error ? <p className="type-meta text-fail-strong">{paste.error}</p> : null}
          </div>
          <div className="flex justify-end gap-2">
            <Button type="button" variant="secondary" onClick={() => setPaste(null)}>
              {t("desktop.cancel")}
            </Button>
            <Button type="submit" disabled={!paste.yaml.trim()}>
              {t("desktop.pasteAdd")}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  )
}
