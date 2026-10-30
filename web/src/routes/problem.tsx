// What one kind of problem means: the page its `type` URI names (RFC 9457, section 3.1.1, "When
// dereferenced, it SHOULD provide human-readable documentation for the problem type"). One short
// page per kind, in the reader's language: the rule behind it, what to do, and where Kubernetes
// documents the rule. A problem's raw layer links here.
import { getRouteApi } from "@tanstack/react-router"
import ArrowSquareOutIcon from "~icons/ph/arrow-square-out"

import type { Schemas } from "@/api/client"
import { NotFound } from "@/components/not-found"
import { useT } from "@/i18n"
import { isKind } from "@/model/problem"

const route = getRouteApi("/problems/$kind")

const docs = "https://kubernetes.io/docs"

/** Where Kubernetes documents the rule behind a kind, when it is the cluster's. */
const references: Partial<Record<Schemas.Kind, { title: string; href: string }[]>> = {
  unauthenticated: [
    { title: "Authenticating", href: `${docs}/reference/access-authn-authz/authentication/` },
  ],
  forbidden: [
    { title: "Using RBAC Authorization", href: `${docs}/reference/access-authn-authz/rbac/` },
  ],
  "quota-exceeded": [
    { title: "Resource Quotas", href: `${docs}/concepts/policy/resource-quotas/` },
  ],
  "policy-denied": [
    {
      title: "Dynamic Admission Control",
      href: `${docs}/reference/access-authn-authz/extensible-admission-controllers/`,
    },
  ],
  "log-unavailable": [
    { title: "Logging Architecture", href: `${docs}/concepts/cluster-administration/logging/` },
  ],
  conflict: [
    {
      title: "Resource versions",
      href: `${docs}/reference/using-api/api-concepts/#resource-versions`,
    },
  ],
  "system-namespace": [
    {
      title: "Initial namespaces",
      href: `${docs}/concepts/overview/working-with-objects/namespaces/#initial-namespaces`,
    },
  ],
  "too-many-requests": [
    {
      title: "API Priority and Fairness",
      href: `${docs}/concepts/cluster-administration/flow-control/`,
    },
  ],
}

export function ProblemPage() {
  const t = useT()
  const { kind } = route.useParams()
  if (!isKind(kind)) return <NotFound />
  const links = references[kind] ?? []
  return (
    <article className="flex max-w-2xl flex-col gap-6 px-4 py-6 pb-16 md:px-8">
      <header className="flex flex-col gap-1">
        <span className="type-meta text-muted-foreground">{t("problemPage.label")}</span>
        <h1 className="type-title">{t(`problem.title.${kind}`)}</h1>
        <code className="type-meta text-muted-foreground" translate="no">
          /problems/{kind}
        </code>
      </header>
      <section className="flex flex-col gap-2">
        <h2 className="text-sm font-medium">{t("problemPage.rule")}</h2>
        <p className="text-sm leading-relaxed">{t(`problemPage.rule.${kind}`)}</p>
      </section>
      <section className="flex flex-col gap-2">
        <h2 className="text-sm font-medium">{t("problem.whatToDo")}</h2>
        <p className="text-sm leading-relaxed">{t(`problem.fix.${kind}`)}</p>
      </section>
      {links.length > 0 ? (
        <section className="flex flex-col gap-2">
          <h2 className="text-sm font-medium">{t("problemPage.kubernetes")}</h2>
          <ul className="flex flex-col gap-1 text-sm">
            {links.map((link) => (
              <li key={link.href}>
                <a
                  href={link.href}
                  target="_blank"
                  rel="noreferrer"
                  className="inline-flex items-center gap-1 underline underline-offset-2"
                >
                  <span lang="en">{link.title}</span>
                  <ArrowSquareOutIcon aria-hidden="true" className="size-3.5" />
                  <span className="sr-only"> {t("common.newTab")}</span>
                </a>
              </li>
            ))}
          </ul>
        </section>
      ) : null}
    </article>
  )
}
