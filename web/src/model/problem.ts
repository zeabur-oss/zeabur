// A server problem in the reader's language, in the layers it is shown in: what happened (the
// keyed `summary`, naming the object), why (the keyed `why`, the rule with this occurrence's
// numbers), what to do (the keyed `fixKey` when the server wrote a fix for the occasion, else the
// kind's fix from the catalogue), then the evidence and the raw answer, folded. The server also
// writes `title`, `detail` and `fix` in English (RFC 9457, for other clients); `detail` says in
// English what the summary says, so it moves into the folded details, unless it is the only
// place that says what went wrong.
import type { Problem, Schemas } from "@/api/client"
import { writeSentence, type Locale, type Translate } from "@/i18n"
import { en } from "@/i18n/en"

type Kind = Schemas["Kind"]

/** Every kind the catalogue has a title and a fix for. */
const kinds = [
  "bad-request",
  "unauthenticated",
  "signed-out",
  "forbidden",
  "elevation-required",
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

/**
 * Kinds whose `detail` says what no key does yet: which request was malformed, which values the
 * cluster rejected, what was already written before a conflict, which tool manages the object,
 * what failed inside Zeabur. A refusal is said by `denied` when it is there, and by its `detail`
 * only without it.
 */
const detailExplains = new Set<string>([
  "bad-request",
  "invalid",
  "conflict",
  "managed",
  "internal",
] satisfies Kind[])

export interface ProblemText {
  /** What happened, naming the object. */
  headline: string
  /** Why: the rule, with this occurrence's numbers. */
  why?: string
  /** `why` is the server's English sentence, not a keyed one. */
  whyInEnglish?: boolean
  /** Words the problem quotes as they were said (a policy's answer). */
  quote?: string
  /** What to do, in one sentence. */
  fix: string
  /** The server's English sentences that the layers above already say, folded with
   * kube-apiserver's answer. */
  folded: string[]
}

export function isKind(kind: string): kind is Kind {
  return known.has(kind)
}

export function problemText(t: Translate, locale: Locale, problem: Problem): ProblemText {
  const kind = problem.kind
  // A problem made in this page (no answer, a proxy's error) or a kind newer than this page.
  if (!isKind(kind) || !problem.type.startsWith("/problems/"))
    return {
      headline: problem.title,
      why: problem.detail,
      whyInEnglish: locale !== "en",
      fix: problem.fix,
      folded: [],
    }
  const headline =
    writeSentence(t, problem.summary) ?? writeSentence(t, { key: `problem.summary.${kind}` })
  const keyedWhy = problem.why ? writeSentence(t, problem.why) : undefined
  const refusal = kind === "forbidden" || kind === "elevation-required"
  const detailShown =
    !keyedWhy && (detailExplains.has(kind) || (refusal && problem.denied === undefined))
  // A fix written for the occasion without a key is English only: shown in English, else folded.
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

/**
 * What kube-apiserver said about the form's field at `path` (`spec.replicas`), for the form to mark
 * it: a cause about that field, about one inside it, or about the object it is in
 * (`…resources.requests` marks `…resources.requests.cpu`).
 */
export function refusedField(
  fields: readonly Schemas["FieldCause"][],
  path: string,
): string | undefined {
  const within = (inner: string, outer: string) =>
    inner === outer || inner.startsWith(`${outer}.`) || inner.startsWith(`${outer}[`)
  const cause = fields.find((f) => within(f.field, path) || within(path, f.field))
  return cause ? (cause.message ?? cause.reason ?? cause.field) : undefined
}

/**
 * Whether a problem is the cluster's refusal (a missing permission, a normal state) rather than an
 * error. Zeabur's own 403s ("only from the Zeabur page", "a plan made by someone else") name no
 * refused request and carry no answer from kube-apiserver; they stay errors. A quota's or a
 * policy's refusal is not about rights: admin rights would not change it.
 */
export function isRefusal(problem: Problem): boolean {
  if (problem.kind === "elevation-required") return true
  return problem.kind === "forbidden" && Boolean(problem.kubernetes || problem.denied)
}
