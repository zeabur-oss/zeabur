import { useState } from "react"
import XIcon from "~icons/ph/x"

import { namespaceQuery, queryClient } from "@/api/queries"
import { NamespaceAccess } from "@/components/namespace-access"
import { usePlanner } from "@/components/plan/planner"
import { permission, RefusedHint } from "@/components/refused"
import { Button } from "@zeabur/design-system/components/button"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogTitle,
} from "@zeabur/design-system/components/dialog"
import { useT } from "@/i18n"
import { MANAGE_VERB, mayAdopt } from "@/model/project"

/** Whether the person may adopt it or let it go, and whether the policy guards its labels. */
async function adoptHint(namespace: string): Promise<{ allowed: boolean; guarded: boolean }> {
  try {
    const ns = await queryClient.fetchQuery({ ...namespaceQuery(namespace), staleTime: 0 })
    return { allowed: mayAdopt(ns), guarded: ns.labelsGuarded }
  } catch {
    // The hint could not be read; the plan and its dry run will say.
    return { allowed: true, guarded: false }
  }
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
  return (
    <>
      <Button size="sm" variant={variant} disabled={checking} onClick={() => void start()}>
        {label ?? (adopted ? t("action.unadopt") : t("action.adopt"))}
      </Button>
      {planner.dialog}
      {help ? (
        <AccessDialog
          namespace={namespace}
          adopted={adopted}
          manage={help.guarded}
          guarded={help.guarded}
          onClose={() => setHelp(undefined)}
          what={adopted ? t("refused.unadopt", namespace) : t("refused.adopt", namespace)}
          retry={() => {
            setHelp(undefined)
            void start()
          }}
        />
      ) : null}
    </>
  )
}

export function AccessDialog({
  namespace,
  adopted,
  manage,
  guarded,
  onClose,
  what,
  retry,
}: {
  namespace: string
  adopted: boolean
  manage: boolean
  guarded: boolean
  onClose: () => void
  what: string
  retry?: () => void
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
          <RefusedHint
            hint="denied"
            what={what}
            missing={
              manage
                ? [permission("patch", "namespaces"), permission(MANAGE_VERB, "namespaces")]
                : [permission("patch", "namespaces")]
            }
            retry={retry}
          />
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
