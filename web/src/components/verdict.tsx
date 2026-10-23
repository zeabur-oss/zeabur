import type { ReactNode } from "react"

import { StatusDot } from "@/components/status"
import { useT } from "@/i18n"
import { DOMAINS_NAMED, type Headline, type Reach, type Trouble } from "@/model/verdict"
import { useStatusWord } from "@/components/status"
import type { Status } from "@/model/service"
import { reachHealth } from "@/model/severity"

export function useHeadline() {
  const t = useT()
  const word = useStatusWord()
  const trouble = (what: Trouble) => {
    switch (what.as) {
      case "reason":
        return t(`reason.${what.word}`)
      case "status":
        return word({ health: "fail", word: what.word } satisfies Status)
      case "healthCheck":
        return t("verdict.trouble.healthCheck")
      case "quota":
        return t("verdict.trouble.quota")
      case "unreachable":
        return t("verdict.trouble.unreachable", what.domain)
    }
  }
  // Running, deploying and stopped say the status word the header and the project's cards use.
  return (headline: Headline, status: Status): string =>
    headline.as === "problem" ? t("verdict.problem", trouble(headline.trouble)) : word(status)
}

/** Whether people reach the service's domains, in one line under the verdict; `more` leads to
 * where a domain is set up. */
export function ReachLine({ reach, more }: { reach: Reach; more?: ReactNode }) {
  const t = useT()
  const named = reach.domains.slice(0, DOMAINS_NAMED)
  const extra = reach.domains.length - named.length
  const list = t("common.list", extra > 0 ? [...named, t("verdict.more", extra)] : named)
  const text = (() => {
    switch (reach.as) {
      case "reachable":
        return reach.browser
          ? t("verdict.reach.browserReachable", list)
          : t("verdict.reach.reachable", list)
      case "unreachable":
        return reach.browser
          ? t("verdict.reach.browserUnreachable", list)
          : t("verdict.reach.unreachable", list)
      case "unknown":
        return t("verdict.reach.unknown", list)
      case "issuing":
        return t("verdict.reach.issuing", list, reach.domains.length)
      case "invalid":
      case "elsewhere":
        return t("verdict.reach.domain", list, t(`domain.${reach.as}`))
    }
  })()
  return (
    <p className="mt-1 flex items-start gap-1.5 text-sm text-secondary" role="status">
      <span className="flex h-5 items-center">
        <StatusDot health={reachHealth[reach.as]} />
      </span>
      <span className="flex min-w-0 flex-wrap items-center gap-x-2">
        <span className="min-w-0">{text}</span>
        {more}
      </span>
    </p>
  )
}

/** The words of the reach line's way to Networking, when a domain there needs setting up. */
export function reachSetup(reach: Reach) {
  if (reach.record) return "verdict.reach.seeRecord" as const
  return reach.as === "invalid" || reach.as === "elsewhere" || reach.as === "issuing"
    ? ("verdict.reach.seeDetails" as const)
    : undefined
}
