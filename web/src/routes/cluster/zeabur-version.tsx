import { useQuery } from "@tanstack/react-query"
import ArrowSquareOutIcon from "~icons/ph/arrow-square-out"

import type { Schemas } from "@/api/client"
import { zeaburQuery } from "@/api/queries"
import { StatusPill } from "@/components/status"
import { useT } from "@/i18n"
import { load, save } from "@/lib/stores"
import { findNewer, updateCheckQuery, type Newer } from "@/model/update-check"

export function useNewer(): { zeabur?: Schemas.ZeaburVersion; newer?: Newer } {
  const zeabur = useQuery(zeaburQuery).data
  const checked = useQuery(
    updateCheckQuery((url, init) => fetch(url, init), { load, save }, zeabur?.updateCheck === true),
  ).data
  return { zeabur, newer: zeabur && checked ? findNewer(checked, zeabur.version) : undefined }
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
