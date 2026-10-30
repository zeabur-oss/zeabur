// The server's `title`, `detail` and `fix` are English (RFC 9457, for other clients); `detail` is
// folded unless it is the only place that says what went wrong.
import type { Problem, Schemas } from "@/api/client"
import { writeSentence, type Locale, type Translate } from "@/i18n"
import { en } from "@/i18n/en"

type Kind = Schemas.Kind

const kinds = [
  "bad-request",
  "unauthenticated",
  "signed-out",
  "forbidden",
  "quota-exceeded",
  "policy-denied",
  "not-found",
  "log-unavailable",
  "conflict",
  "managed",
  "plan-expired",
  "system-namespace",
  "invalid",
  "too-many-requests",
  "unavailable",
  "internal",
] as const satisfies Kind[]

const known = new Set<string>(kinds)

/** Kinds whose `detail` says what no key does yet. */
const detailExplains = new Set<string>([
  "bad-request",
  "invalid",
  "conflict",
  "managed",
  "internal",
] satisfies Kind[])

export interface ProblemText {
  headline: string
  why?: string
  whyInEnglish?: boolean
  /** A policy's answer, quoted verbatim. */
  quote?: string
  fix: string
  folded: string[]
}

export function isKind(kind: string): kind is Kind {
  return known.has(kind)
}

export function problemText(t: Translate, locale: Locale, problem: Problem): ProblemText {
  const kind = problem.kind
  // A problem made in this page, or a kind newer than this build.
  if (!isKind(kind) || !problem.type.startsWith("/problems/"))
    return {
      headline:
        (problem.type === "about:blank" ? writeSentence(t, problem.summary) : undefined) ??
        problem.title,
      why: problem.detail,
      whyInEnglish: locale !== "en",
      fix: (problem.fixKey ? writeSentence(t, problem.fixKey) : undefined) ?? problem.fix,
      folded: [],
    }
  const headline =
    writeSentence(t, problem.summary) ?? writeSentence(t, { key: `problem.summary.${kind}` })
  const keyedWhy = problem.why ? writeSentence(t, problem.why) : undefined
  const refusal = kind === "forbidden"
  const detailShown =
    !keyedWhy && (detailExplains.has(kind) || (refusal && problem.denied === undefined))
  // An unkeyed fix is English only: shown in English, else folded.
  const ownFix = !problem.fixKey && problem.fix !== en[`problem.fix.${kind}`]
  const keyedFix = problem.fixKey ? writeSentence(t, problem.fixKey) : undefined
  const fix = keyedFix ?? (ownFix && locale === "en" ? problem.fix : t(`problem.fix.${kind}`))
  const quote = kind === "policy-denied" ? problem.why?.params?.message : undefined
  return {
    headline: headline ?? t(`problem.title.${kind}`),
    why: keyedWhy ?? (detailShown ? problem.detail : undefined),
    whyInEnglish: !keyedWhy && detailShown && locale !== "en",
    quote: quote || undefined,
    fix,
    folded: [
      ...(detailShown ? [] : [problem.detail]),
      ...(ownFix && locale !== "en" ? [problem.fix] : []),
    ].filter(Boolean),
  }
}

/** A cause about `path`, a field inside it, or the object it is in. */
export function refusedField(
  fields: readonly Schemas.FieldCause[],
  path: string,
): string | undefined {
  const within = (inner: string, outer: string) =>
    inner === outer || inner.startsWith(`${outer}.`) || inner.startsWith(`${outer}[`)
  const cause = fields.find((f) => within(f.field, path) || within(path, f.field))
  return cause ? (cause.message ?? cause.reason ?? cause.field) : undefined
}

/**
 * Zeabur's own 403s ("only from the Zeabur page", "a plan made by someone else") carry no
 * kube-apiserver answer and stay errors; quota and policy refusals are not about rights.
 */
export function isRefusal(problem: Problem): boolean {
  return problem.kind === "forbidden" && Boolean(problem.kubernetes || problem.denied)
}
