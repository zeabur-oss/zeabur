import { useQuery } from "@tanstack/react-query"
import { useState } from "react"
import ArrowSquareOutIcon from "~icons/ph/arrow-square-out"

import type { Schemas } from "@/api/client"
import { zeaburQuery } from "@/api/queries"
import { Command } from "@/components/bits"
import { StatusPill } from "@/components/status"
import { Button } from "@zeabur-oss/design-system/components/button"
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@zeabur-oss/design-system/components/dialog"
import { useI18n, useT } from "@/i18n"
import { docsUrl } from "@/lib/docs"
import { updateQuery, type Newer } from "@/model/update-check"

export function useNewer(): {
  zeabur?: Schemas.ZeaburVersion
  newer?: Newer
  checked?: boolean
} {
  const zeabur = useQuery(zeaburQuery).data
  const on = zeabur?.updateCheck === true
  const update = useQuery(updateQuery(on)).data
  // A cached answer says nothing once the check is turned off.
  return { zeabur, newer: (on && update?.newer) || undefined, checked: on && update !== undefined }
}

/** "Zeabur v0.1.1", and a pill when a newer release is out. */
export function ZeaburVersionBadges() {
  const t = useT()
  const { zeabur, newer } = useNewer()
  if (!zeabur) return null
  return (
    <>
      <span className="type-mono text-muted-foreground" translate="no">
        Zeabur v{zeabur.version}
      </span>
      {newer ? (
        newer.security.length > 0 ? (
          <StatusPill health="fail">{t("update.securityBadge")}</StatusPill>
        ) : (
          <StatusPill health="progress">{t("cluster.available", newer.latest.version)}</StatusPill>
        )
      ) : null}
    </>
  )
}

export function OutLink({ href, children }: { href: string; children: string }) {
  const t = useT()
  return (
    <a
      href={href}
      target="_blank"
      rel="noreferrer"
      className="inline-flex items-center gap-1 text-foreground hover:underline"
    >
      {children}
      <ArrowSquareOutIcon aria-hidden="true" className="size-3" />
      <span className="sr-only">{t("common.newTab")}</span>
    </a>
  )
}

export const HOW = {
  installer: "update.how.installer",
  helm: "update.how.helm",
  "argo-cd": "update.how.argoCd",
  flux: "update.how.flux",
  zeaburctl: "update.how.zeaburctl",
} as const satisfies Record<Schemas.InstallMethod, string>

/** The commands of an upgrade to `version`, where the way Zeabur was installed has them. */
export function upgradeCommands(by: Schemas.InstallMethod | undefined, version: string): string[] {
  switch (by) {
    case "installer":
      return ["sudo zeabur-installer plan\nsudo zeabur-installer apply"]
    case "helm":
      return [
        `helm upgrade zeabur oci://ghcr.io/zeabur-oss/charts/zeabur \\\n  --version ${version} \\\n  --namespace zeabur --reset-then-reuse-values`,
      ]
    default:
      return []
  }
}

/** "How to upgrade", opening the steps for the way Zeabur was installed. */
export function UpgradeButton({
  zeabur,
  newer,
  size = "sm",
}: {
  zeabur: Schemas.ZeaburVersion
  newer: Newer
  size?: "xs" | "sm"
}) {
  const t = useT()
  const [open, setOpen] = useState(false)
  return (
    <>
      <Button variant="outline" size={size} onClick={() => setOpen(true)}>
        {t("update.guide")}
      </Button>
      {open ? <UpgradeDialog zeabur={zeabur} newer={newer} onClose={() => setOpen(false)} /> : null}
    </>
  )
}

function UpgradeDialog({
  zeabur,
  newer,
  onClose,
}: {
  zeabur: Schemas.ZeaburVersion
  newer: Newer
  onClose: () => void
}) {
  const t = useT()
  const { locale } = useI18n()
  const by = zeabur.installedBy ?? undefined
  const local = by === "zeaburctl"
  const product = local ? "zeaburctl" : "Zeabur"
  const security = newer.security.length > 0
  return (
    <Dialog open onOpenChange={(next) => (next ? undefined : onClose())}>
      <DialogContent>
        <DialogHeader closeLabel={t("common.close")}>
          <DialogTitle>
            {security
              ? t("update.security", product, zeabur.version)
              : t("update.out", product, newer.latest.version)}
          </DialogTitle>
          <DialogDescription>
            {t("common.sentences", [
              t("update.current", zeabur.version),
              security
                ? t(
                    "update.fixedIn",
                    newer.security.map((r) => `v${r.version}`).join(t("common.sep")),
                  )
                : undefined,
            ])}
          </DialogDescription>
        </DialogHeader>
        <DialogBody>
          <div className="flex flex-col gap-3 type-body">
            {by ? <p className="text-pretty">{t(HOW[by])}</p> : null}
            {upgradeCommands(by, newer.latest.version).map((command) => (
              <Command key={command} text={command} />
            ))}
            <span className="flex flex-wrap gap-x-4 gap-y-2">
              <OutLink href={newer.latest.url}>{t("update.notesOf", newer.latest.version)}</OutLink>
              <OutLink href={docsUrl(locale, local ? "reference/releases" : "upgrade/steps")}>
                {t("update.guideFull")}
              </OutLink>
            </span>
          </div>
        </DialogBody>
        <DialogFooter>
          <Button variant="ghost" size="sm" onClick={onClose}>
            {t("common.close")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
