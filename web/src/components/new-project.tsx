import { useNavigate } from "@tanstack/react-router"
import { useState } from "react"
import PlusIcon from "~icons/ph/plus"
import XIcon from "~icons/ph/x"

import type { Schemas } from "@/api/client"
import { FormField } from "@/components/form-field"
import { usePlanner } from "@/components/plan/planner"
import { permission, RefusedHint, useHintOffersElevation } from "@/components/refused"
import { useT } from "@/i18n"
import { DISPLAY_NAME_MAX, newProjectProblem } from "@/model/project"
import { Button } from "@zeabur/design-system/components/button"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogTitle,
} from "@zeabur/design-system/components/dialog"
import { Input } from "@zeabur/design-system/components/input"

/** "New project", for those who may create namespaces, or may once they act as admin. */
export function NewProjectButton({
  canCreate,
  taken,
}: {
  canCreate: Schemas["Permission"] | undefined
  taken: string[]
}) {
  const t = useT()
  const [open, setOpen] = useState(false)
  const elevate = useHintOffersElevation(canCreate)
  if (canCreate === "denied" && !elevate) return null
  return (
    <>
      <Button size="sm" data-testid="new-project" onClick={() => setOpen(true)}>
        <PlusIcon aria-hidden="true" data-icon="inline-start" />
        {t("projects.new")}
      </Button>
      {open ? (
        <NewProjectDialog canCreate={canCreate} taken={taken} onClose={() => setOpen(false)} />
      ) : null}
    </>
  )
}

function NewProjectDialog({
  canCreate,
  taken,
  onClose,
}: {
  canCreate: Schemas["Permission"] | undefined
  taken: string[]
  onClose: () => void
}) {
  const t = useT()
  const navigate = useNavigate()
  const [name, setName] = useState("")
  const [displayName, setDisplayName] = useState("")
  const [tried, setTried] = useState(false)
  const problem = newProjectProblem(name, taken)
  const tooLong = [...displayName.trim()].length > DISPLAY_NAME_MAX
  const planner = usePlanner(name, () => {
    onClose()
    void navigate({ to: "/projects/$project", params: { project: name } })
  })
  const review = () => {
    setTried(true)
    if (problem || tooLong) return
    const display = displayName.trim()
    planner.plan({
      title: t("projects.planNew", name),
      changes: [{ op: "createProject", displayName: display === "" ? null : display }],
    })
  }
  return (
    <>
      <Dialog open onOpenChange={(next) => (next ? undefined : onClose())}>
        <DialogContent>
          <div className="flex items-center gap-2 border-b px-5 py-3">
            <div className="min-w-0 flex-1 truncate">
              <DialogTitle>{t("projects.new")}</DialogTitle>
            </div>
            <DialogClose
              render={<Button variant="ghost" size="icon-sm" aria-label={t("common.close")} />}
            >
              <XIcon aria-hidden="true" />
            </DialogClose>
          </div>
          <form
            className="flex flex-col gap-4 px-5 py-4"
            onSubmit={(event) => {
              event.preventDefault()
              review()
            }}
          >
            <p className="type-meta text-muted-foreground">{t("projects.newBody")}</p>
            <RefusedHint
              hint={canCreate}
              what={name ? t("refused.createProject", name) : t("projects.new")}
              missing={[permission("create", "namespaces")]}
            />
            <FormField
              label={t("projects.newName")}
              hint={t("projects.newNameHint")}
              errors={tried && problem ? [t(`projects.problem.${problem}`)] : []}
            >
              <Input
                size="sm"
                mono
                value={name}
                autoFocus
                autoComplete="off"
                spellCheck={false}
                onChange={(event) => setName(event.target.value.trim().toLowerCase())}
              />
            </FormField>
            <FormField
              label={t("projects.newDisplayName")}
              hint={t("projects.newDisplayNameHint")}
              errors={tried && tooLong ? [t("projectSettings.tooLong", DISPLAY_NAME_MAX)] : []}
            >
              <Input
                size="sm"
                value={displayName}
                autoComplete="off"
                onChange={(event) => setDisplayName(event.target.value)}
              />
            </FormField>
            <div className="flex justify-end gap-2">
              <Button type="button" variant="ghost" size="sm" onClick={onClose}>
                {t("edit.cancel")}
              </Button>
              <Button type="submit" size="sm">
                {t("edit.review")}
              </Button>
            </div>
          </form>
        </DialogContent>
      </Dialog>
      {planner.dialog}
    </>
  )
}
