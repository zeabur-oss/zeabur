import { useState } from "react"
import DotsThreeIcon from "~icons/ph/dots-three"
import XIcon from "~icons/ph/x"

import { namespaceQuery, queryClient } from "@/api/queries"
import { NamespaceAccess } from "@/components/namespace-access"
import { usePlanner } from "@/components/plan/planner"
import { Button } from "@zeabur-oss/design-system/components/button"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogTitle,
} from "@zeabur-oss/design-system/components/dialog"
import { Menu, MenuContent, MenuItem, MenuTrigger } from "@zeabur-oss/design-system/components/menu"
import { useT } from "@/i18n"
import { mayAdopt } from "@/model/project"

/** Whether the person may adopt it or let it go, and whether the policy guards its labels. */
async function adoptHint(namespace: string): Promise<{ allowed: boolean; guarded: boolean }> {
  try {
    const ns = await queryClient.query({ ...namespaceQuery(namespace), staleTime: 0 })
    return { allowed: mayAdopt(ns), guarded: ns.labelsGuarded }
  } catch {
    // The hint could not be read; the plan and its dry run will say.
    return { allowed: true, guarded: false }
  }
}

/** Adopting or letting go: the plan, or what rights it needs when the person lacks them. */
function useAdopt(namespace: string, adopted: boolean) {
  const t = useT()
  const planner = usePlanner(namespace)
  const [help, setHelp] = useState<{ guarded: boolean }>()
  const [checking, setChecking] = useState(false)
  const start = async () => {
    setChecking(true)
    const { allowed, guarded } = await adoptHint(namespace)
    setChecking(false)
    if (!allowed) {
      setHelp({ guarded })
      return
    }
    planner.plan(
      adopted
        ? { title: t("title.unadopt", namespace), changes: [{ op: "unadopt" }] }
        : { title: t("title.adopt", namespace), changes: [{ op: "adopt" }] },
    )
  }
  const dialogs = (
    <>
      {planner.dialog}
      {help ? (
        <AccessDialog
          namespace={namespace}
          adopted={adopted}
          manage={help.guarded}
          guarded={help.guarded}
          onClose={() => setHelp(undefined)}
        />
      ) : null}
    </>
  )
  return { start: () => void start(), checking, dialogs }
}

export function AdoptButton({
  namespace,
  adopted,
  variant = "outline",
  label,
}: {
  namespace: string
  adopted: boolean
  variant?: "outline" | "destructive"
  label?: string
}) {
  const t = useT()
  const adopt = useAdopt(namespace, adopted)
  return (
    <>
      <Button size="sm" variant={variant} disabled={adopt.checking} onClick={adopt.start}>
        {label ?? (adopted ? t("action.unadopt") : t("action.adopt"))}
      </Button>
      {adopt.dialogs}
    </>
  )
}

/** A namespace row's other actions: adopting it is not the page's main path. */
export function AdoptMenu({ namespace }: { namespace: string }) {
  const t = useT()
  const adopt = useAdopt(namespace, false)
  return (
    <>
      <Menu>
        <MenuTrigger
          render={
            <Button
              variant="ghost"
              size="icon-sm"
              disabled={adopt.checking}
              aria-label={t("projects.rowMenu", namespace)}
            />
          }
        >
          <DotsThreeIcon aria-hidden="true" />
        </MenuTrigger>
        <MenuContent align="end">
          <MenuItem onClick={adopt.start}>{t("action.adopt")}</MenuItem>
        </MenuContent>
      </Menu>
      {adopt.dialogs}
    </>
  )
}

export function AccessDialog({
  namespace,
  adopted,
  manage,
  guarded,
  onClose,
}: {
  namespace: string
  adopted: boolean
  manage: boolean
  guarded: boolean
  onClose: () => void
}) {
  const t = useT()
  return (
    <Dialog open onOpenChange={(open) => (open ? undefined : onClose())}>
      <DialogContent variant="wide">
        <div className="flex items-center gap-2 border-b px-5 py-3">
          <div className="min-w-0 flex-1 truncate">
            <DialogTitle>{t("access.title", namespace)}</DialogTitle>
          </div>
          <DialogClose
            render={<Button variant="ghost" size="icon-sm" aria-label={t("common.close")} />}
          >
            <XIcon aria-hidden="true" />
          </DialogClose>
        </div>
        <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto px-5 py-4">
          <NamespaceAccess
            namespace={namespace}
            adopted={adopted}
            manage={manage}
            guarded={guarded}
          />
        </div>
      </DialogContent>
    </Dialog>
  )
}
