import { StatusDot } from "@/components/status"
import { useT } from "@/i18n"
import { DOMAINS_NAMED, type Headline, type Reach, type Trouble } from "@/model/verdict"
import { useStatusWord } from "@/components/status"
import type { Status } from "@/model/service"

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

const reachHealth = { reachable: "ok", unreachable: "fail", unknown: "idle" } as const

/** Whether people reach the service's domains, in one line under the verdict. */
export function ReachLine({ reach }: { reach: Reach }) {
  const t = useT()
  const named = reach.domains.slice(0, DOMAINS_NAMED)
  const more = reach.domains.length - named.length
  const list = t("common.list", more > 0 ? [...named, t("verdict.more", more)] : named)
  const text =
    reach.as === "reachable"
      ? reach.browser
        ? t("verdict.reach.browserReachable", list)
        : t("verdict.reach.reachable", list)
      : reach.as === "unreachable"
        ? reach.browser
          ? t("verdict.reach.browserUnreachable", list)
          : t("verdict.reach.unreachable", list)
        : t("verdict.reach.unknown", list)
  return (
    <p className="mt-1 flex items-center gap-1.5 text-sm text-secondary" role="status">
      <StatusDot health={reachHealth[reach.as]} />
      <span className="min-w-0">{text}</span>
    </p>
  )
}
