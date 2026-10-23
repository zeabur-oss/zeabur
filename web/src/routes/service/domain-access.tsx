import { useQuery } from "@tanstack/react-query"
import { useId, useState } from "react"

import { objectQuery } from "@/api/queries"
import type { Schemas } from "@/api/client"
import { FormField } from "@/components/form-field"
import { KubeDetails } from "@/components/kube-details"
import { PlanDrawer, ReviewFooter, ReviewForm } from "@/components/plan/plan-drawer"
import { usePlanStep } from "@/components/plan/planner"
import { useT } from "@/i18n"
import type { Translate } from "@/i18n"
import type { Service } from "@/model/service"
import { Button } from "@zeabur/design-system/components/button"
import { Input } from "@zeabur/design-system/components/input"
import { Label } from "@zeabur/design-system/components/label"
import { RadioGroup, RadioOption } from "@zeabur/design-system/components/radio"
import { Select } from "@zeabur/design-system/components/select"
import { Switch } from "@zeabur/design-system/components/switch"

type Change = Schemas["Change"]

const MIDDLEWARES = "traefik.ingress.kubernetes.io/router.middlewares"
const HOST = /^[a-z0-9]([-a-z0-9]*[a-z0-9])?(\.[a-z0-9]([-a-z0-9]*[a-z0-9])?)+$/

/** The target of the `redirectRegex` Zeabur wrote, `replacement: https://<host>/${2}`, read from
 * the Middleware's YAML. */
function redirectOf(yaml: string | undefined): { to: string; permanent: boolean } | undefined {
  const to = /replacement:\s*["']?https:\/\/([^/"'\s]+)\//.exec(yaml ?? "")?.[1]
  return to ? { to, permanent: /permanent:\s*true/.test(yaml ?? "") } : undefined
}

const middlewareAddress = (name: string) => ({
  group: "traefik.io",
  version: "v1alpha1",
  resource: "middlewares",
  name,
})

type Mode = "serve" | "redirect"

interface RedirectForm {
  mode: Mode
  /** Unset until typed: the target Zeabur wrote, once it loads. */
  to?: string
  permanent?: boolean
}

interface PasswordForm {
  on: boolean
  /** A password already set stays until Change is chosen. */
  changing: boolean
  username: string
  password: string
}

function redirectProblem(t: Translate, target: string, host: string): string | undefined {
  if (!HOST.test(target)) return t("domainAccess.badTo")
  return target === host ? t("domainAccess.sameTo") : undefined
}

/** Traefik reads `user:hash` lines; bcrypt reads at most 72 bytes. */
function userProblems(t: Translate, form: PasswordForm) {
  return {
    username: /^[^\s:]+$/.test(form.username) ? undefined : t("domainAccess.badUser"),
    password:
      form.password.length < 8 || new TextEncoder().encode(form.password).length > 72
        ? t("domainAccess.badPassword")
        : undefined,
  }
}

/** The redirect Zeabur wrote for `host`, with its target once the Middleware loads. */
function useExisting(project: string, route: Schemas["Route"], host: string) {
  const redirect = route.redirects?.find((r) => r.host === host)
  const current = useQuery({
    ...objectQuery(project, middlewareAddress(redirect?.middleware ?? "")),
    enabled: redirect !== undefined,
  })
  const was = redirectOf(current.data?.yaml)
  return redirect && { middleware: redirect.middleware, to: was?.to, permanent: was?.permanent }
}

type Existing = ReturnType<typeof useExisting>

/** The form's target and kind, or the existing redirect's until they are changed. */
function resolved(form: RedirectForm, existing: Existing) {
  const shown = form.to ?? existing?.to ?? ""
  return {
    shown,
    to: shown.trim().toLowerCase(),
    permanent: form.permanent ?? existing?.permanent ?? true,
  }
}

function redirectChanges(
  route: Schemas["RouteRef"],
  workload: Schemas["WorkloadRef"],
  host: string,
  existing: Existing,
  next: { mode: Mode; to: string; permanent: boolean },
): Change[] {
  const base = { op: "redirect", route, workload, hostname: host } as const
  if (next.mode === "serve") return existing ? [{ ...base, permanent: true }] : []
  const same = existing && next.to === existing.to && next.permanent === existing.permanent
  return same ? [] : [{ ...base, to: next.to, permanent: next.permanent }]
}

function passwordChanges(
  route: Schemas["RouteRef"],
  form: PasswordForm,
  protectedNow: boolean,
): Change[] {
  if (form.on && form.changing)
    return [{ op: "basicAuth", route, user: { username: form.username, password: form.password } }]
  return !form.on && protectedNow ? [{ op: "basicAuth", route }] : []
}

function problemsOf(
  t: Translate,
  host: string,
  redirect: { mode: Mode; to: string },
  password: PasswordForm,
) {
  return {
    to: redirect.mode === "redirect" ? redirectProblem(t, redirect.to, host) : undefined,
    ...(password.on && password.changing ? userProblems(t, password) : {}),
  }
}

/** Redirect one domain of an Ingress, or ask for a password on all of its domains. */
export default function DomainAccessDrawer({
  project,
  service,
  route,
  host,
  onClose,
}: {
  project: string
  service: Service
  route: Schemas["Route"]
  host: string
  onClose: () => void
}) {
  const t = useT()
  const step = usePlanStep(project, onClose)
  const existing = useExisting(project, route, host)
  const [redirectForm, setRedirectForm] = useState<RedirectForm>({
    mode: existing ? "redirect" : "serve",
  })
  const target = { mode: redirectForm.mode, ...resolved(redirectForm, existing) }
  const protectedNow = route.basicAuth != null
  const [passwordForm, setPasswordForm] = useState<PasswordForm>({
    on: protectedNow,
    changing: !protectedNow,
    username: "",
    password: "",
  })
  const [tried, setTried] = useState(false)
  const routeRef = { kind: "Ingress", name: route.name } as const
  const workload = { kind: service.kind, name: service.name }
  const others = route.hostnames.filter((h) => h !== host)

  const problems = problemsOf(t, host, target, passwordForm)
  const changes = [
    ...redirectChanges(routeRef, workload, host, existing, target),
    ...passwordChanges(routeRef, passwordForm, protectedNow),
  ]
  const nothing = tried && changes.length === 0
  const review = () => {
    setTried(true)
    if (Object.values(problems).some(Boolean) || changes.length === 0) return
    step.plan({ title: t("domainAccess.planTitle", host), service: workload, changes })
  }
  const shown = (message?: string) => (tried && message ? [message] : [])
  return (
    <PlanDrawer
      step={step}
      onClose={onClose}
      title={t("domainAccess.title", host)}
      description={t("domainAccess.lead")}
      footer={
        <ReviewFooter
          onCancel={onClose}
          onReview={review}
          note={nothing ? t("domainAccess.nothing") : undefined}
        />
      }
    >
      <ReviewForm onReview={review}>
        <RedirectFields
          service={service}
          host={host}
          others={others}
          form={redirectForm}
          onChange={setRedirectForm}
          shownTo={target.shown}
          permanent={target.permanent}
          errors={shown(problems.to)}
        />
        <PasswordFields
          others={others}
          form={passwordForm}
          onChange={setPasswordForm}
          errors={{ username: shown(problems.username), password: shown(problems.password) }}
        />
        <AccessDetails route={route} host={host} redirect={existing?.middleware} />
      </ReviewForm>
    </PlanDrawer>
  )
}

function RedirectFields({
  service,
  host,
  others,
  form,
  onChange,
  shownTo,
  permanent,
  errors,
}: {
  service: Service
  host: string
  others: string[]
  form: RedirectForm
  onChange: (form: RedirectForm) => void
  shownTo: string
  permanent: boolean
  errors: string[]
}) {
  const t = useT()
  return (
    <fieldset className="flex flex-col gap-3">
      <legend className="mb-1.5 text-sm font-medium">{t("domainAccess.goes", host)}</legend>
      <RadioGroup
        value={form.mode}
        onValueChange={(value) => onChange({ ...form, mode: value as Mode })}
      >
        <RadioOption value="serve" label={t("domainAccess.serve", service.name)} />
        <RadioOption value="redirect" label={t("domainAccess.redirect")} />
      </RadioGroup>
      {form.mode === "redirect" ? (
        <div className="flex flex-col gap-4">
          <FormField label={t("domainAccess.to")} hint={t("domainAccess.toHint")} errors={errors}>
            <Input
              size="sm"
              mono
              value={shownTo}
              placeholder={others[0] ?? "example.com"}
              autoComplete="off"
              spellCheck={false}
              onChange={(event) => onChange({ ...form, to: event.target.value })}
            />
          </FormField>
          <FormField label={t("domainAccess.kind")} hint={t("domainAccess.permanentHint")}>
            <Select
              size="sm"
              value={permanent ? "permanent" : "temporary"}
              onValueChange={(value) => onChange({ ...form, permanent: value === "permanent" })}
              items={[
                { value: "permanent", label: t("domainAccess.permanent") },
                { value: "temporary", label: t("domainAccess.temporary") },
              ]}
            />
          </FormField>
        </div>
      ) : null}
    </fieldset>
  )
}

function PasswordFields({
  others,
  form,
  onChange,
  errors,
}: {
  others: string[]
  form: PasswordForm
  onChange: (form: PasswordForm) => void
  errors: { username: string[]; password: string[] }
}) {
  const t = useT()
  const id = useId()
  return (
    <fieldset className="flex flex-col gap-3">
      <legend className="sr-only">{t("domainAccess.password")}</legend>
      <div className="flex items-center gap-2">
        <Switch
          id={id}
          size="sm"
          checked={form.on}
          onCheckedChange={(on) => onChange({ ...form, on })}
        />
        <Label htmlFor={id}>{t("domainAccess.password")}</Label>
      </div>
      <p className="type-meta text-muted-foreground">
        {t("domainAccess.passwordHint")}
        {others.length > 0 ? ` ${t("domainAccess.passwordShared", t("common.list", others))}` : ""}
      </p>
      {form.on && !form.changing ? (
        <div className="flex flex-wrap items-center gap-2">
          <span className="type-meta text-muted-foreground">{t("domainAccess.passwordSet")}</span>
          <Button variant="outline" size="xs" onClick={() => onChange({ ...form, changing: true })}>
            {t("domainAccess.change")}
          </Button>
        </div>
      ) : null}
      {form.on && form.changing ? (
        <div className="grid gap-4 sm:grid-cols-2">
          <FormField label={t("domainAccess.username")} errors={errors.username}>
            <Input
              size="sm"
              value={form.username}
              autoComplete="off"
              spellCheck={false}
              onChange={(event) => onChange({ ...form, username: event.target.value })}
            />
          </FormField>
          <FormField label={t("domainAccess.newPassword")} errors={errors.password}>
            <Input
              size="sm"
              type="password"
              value={form.password}
              autoComplete="new-password"
              onChange={(event) => onChange({ ...form, password: event.target.value })}
            />
          </FormField>
        </div>
      ) : null}
    </fieldset>
  )
}

function AccessDetails({
  route,
  host,
  redirect,
}: {
  route: Schemas["Route"]
  host: string
  redirect?: string
}) {
  const t = useT()
  const basicAuthName = route.basicAuth?.middleware ?? `${route.name}-basic-auth`
  const redirectName = redirect ?? `${route.name}-redirect-${host.replaceAll(".", "-")}`
  return (
    <KubeDetails
      facts={[
        [t("domainAccess.kube.annotation"), `Ingress/${route.name} ${MIDDLEWARES}`],
        [t("domainAccess.kube.redirect"), `Middleware/${redirectName} redirectRegex`],
        [t("domainAccess.kube.password"), `Middleware/${basicAuthName} basicAuth`],
        [t("domainAccess.kube.users"), t("domainAccess.kube.usersValue", basicAuthName)],
      ]}
    />
  )
}
