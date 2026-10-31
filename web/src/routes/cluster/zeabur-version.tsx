import { useQuery } from "@tanstack/react-query"
import ArrowSquareOutIcon from "~icons/ph/arrow-square-out"

import type { Schemas } from "@/api/client"
import { zeaburQuery } from "@/api/queries"
import { StatusPill } from "@/components/status"
import { useI18n, useT } from "@/i18n"
import { docsUrl } from "@/lib/docs"
import { load, save } from "@/lib/stores"
import { findNewer, updateCheckQuery, type Newer, type Release } from "@/model/update-check"

function useNewer(): { zeabur?: Schemas.ZeaburVersion; newer?: Newer } {
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

function OutLink({ href, children }: { href: string; children: string }) {
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

function SecurityLinks({ releases }: { releases: Release[] }) {
  const t = useT()
  return (
    <span translate="no">
      {releases.map((release, i) => (
        <span key={release.version}>
          {i > 0 ? t("common.sep") : null}
          <OutLink href={release.url}>{`v${release.version}`}</OutLink>
        </span>
      ))}
    </span>
  )
}

const HOW = {
  installer: "update.how.installer",
  helm: "update.how.helm",
  "argo-cd": "update.how.argoCd",
  flux: "update.how.flux",
  zeaburctl: "update.how.zeaburctl",
} as const satisfies Record<Schemas.InstallMethod, string>

/** One line under the header: what is out, how to upgrade the way Zeabur was installed, and
 * the releases that fix the running version's vulnerabilities. */
export function ZeaburUpdateLine() {
  const t = useT()
  const { locale } = useI18n()
  const { zeabur, newer } = useNewer()
  if (!zeabur || !newer) return null
  const by = zeabur.installedBy ?? undefined
  const local = by === "zeaburctl"
  const guide = docsUrl(locale, local ? "reference/releases" : "get-started/upgrade")
  const product = local ? "zeaburctl" : "Zeabur"
  const gap = t("common.gap")
  const security = newer.security.length > 0
  return (
    <p className="max-w-prose type-meta text-muted-foreground">
      {security ? (
        t("update.security", product, zeabur.version)
      ) : (
        <>
          {t("update.out", product, newer.latest.version)}
          {gap}
          <OutLink href={newer.latest.url}>{t("update.notes")}</OutLink>
        </>
      )}
      {gap}
      {by ? `${t(HOW[by])}${gap}` : null}
      <OutLink href={guide}>{local ? t("update.download") : t("update.guide")}</OutLink>
      {security ? (
        <>
          {gap}
          {t("update.fixedIn")}
          {t("common.colon")}
          <SecurityLinks releases={newer.security} />
        </>
      ) : null}
    </p>
  )
}
