import { useQuery } from "@tanstack/react-query"
import { useId, useState } from "react"

import { objectQuery } from "@/api/queries"
import type { Schemas } from "@/api/client"
import { FormField } from "@/components/form-field"
import { KubeDetails } from "@/components/kube-details"
import { PlanDrawer } from "@/components/plan/plan-drawer"
import { usePlanStep } from "@/components/plan/planner"
import { useT } from "@/i18n"
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
  const ids = { auth: useId() }
  const step = usePlanStep(project, onClose)
  const redirect = route.redirects?.find((r) => r.host === host)
  const current = useQuery({
    ...objectQuery(project, middlewareAddress(redirect?.middleware ?? "")),
    enabled: redirect !== undefined,
  })
  const was = redirectOf(current.data?.yaml)
  const [mode, setMode] = useState<"serve" | "redirect">(redirect ? "redirect" : "serve")
  const [to, setTo] = useState<string>()
  const [permanent, setPermanent] = useState<boolean>()
  const target = (to ?? was?.to ?? "").trim().toLowerCase()
  const keep = permanent ?? was?.permanent ?? true
  const protectedNow = route.basicAuth != null
  const [auth, setAuth] = useState(protectedNow)
  const [changing, setChanging] = useState(!protectedNow)
  const [username, setUsername] = useState("")
  const [password, setPassword] = useState("")
  const [tried, setTried] = useState(false)
  const others = route.hostnames.filter((h) => h !== host)
  const routeRef = { kind: "Ingress", name: route.name } as const
  const workload = { kind: service.kind, name: service.name }

  const wantsUser = auth && changing
  const problems = {
    to:
      mode !== "redirect"
        ? undefined
        : !HOST.test(target)
          ? t("domainAccess.badTo")
          : target === host
            ? t("domainAccess.sameTo")
            : undefined,
    username: wantsUser && !/^[^\s:]+$/.test(username) ? t("domainAccess.badUser") : undefined,
    password:
      wantsUser && (password.length < 8 || new TextEncoder().encode(password).length > 72)
        ? t("domainAccess.badPassword")
        : undefined,
  }
  const changes: Change[] = []
  if (mode === "redirect" && (!redirect || target !== was?.to || keep !== was?.permanent))
    changes.push({
      op: "redirect",
      route: routeRef,
      workload,
      hostname: host,
      to: target,
      permanent: keep,
    })
  if (mode === "serve" && redirect)
    changes.push({ op: "redirect", route: routeRef, workload, hostname: host, permanent: true })
  if (wantsUser) changes.push({ op: "basicAuth", route: routeRef, user: { username, password } })
  if (!auth && protectedNow) changes.push({ op: "basicAuth", route: routeRef })

  const review = () => {
    setTried(true)
    if (Object.values(problems).some(Boolean) || changes.length === 0) return
    step.plan({ title: t("domainAccess.planTitle", host), service: workload, changes })
  }
  const shown = (message?: string) => (tried && message ? [message] : [])
  const basicAuthName = route.basicAuth?.middleware ?? `${route.name}-basic-auth`
  return (
    <PlanDrawer
      step={step}
      onClose={onClose}
      title={t("domainAccess.title", host)}
      description={t("domainAccess.lead")}
      footer={
        <div className="flex items-center justify-end gap-2">
          {tried && changes.length === 0 ? (
            <span className="mr-auto type-meta text-muted-foreground">
              {t("domainAccess.nothing")}
            </span>
          ) : null}
          <Button variant="ghost" size="sm" onClick={onClose}>
            {t("edit.cancel")}
          </Button>
          <Button size="sm" onClick={review}>
            {t("edit.review")}
          </Button>
        </div>
      }
    >
      <form
        className="flex flex-col gap-6"
        onSubmit={(event) => {
          event.preventDefault()
          review()
        }}
      >
        <fieldset className="flex flex-col gap-3">
          <legend className="mb-1.5 text-sm font-medium">{t("domainAccess.goes", host)}</legend>
          <RadioGroup value={mode} onValueChange={(value) => setMode(value as typeof mode)}>
            <RadioOption value="serve" label={t("domainAccess.serve", service.name)} />
            <RadioOption value="redirect" label={t("domainAccess.redirect")} />
          </RadioGroup>
          {mode === "redirect" ? (
            <div className="flex flex-col gap-4">
              <FormField
                label={t("domainAccess.to")}
                hint={t("domainAccess.toHint")}
                errors={shown(problems.to)}
              >
                <Input
                  size="sm"
                  mono
                  value={to ?? was?.to ?? ""}
                  placeholder={others[0] ?? "example.com"}
                  autoComplete="off"
                  spellCheck={false}
                  onChange={(event) => setTo(event.target.value)}
                />
              </FormField>
              <FormField label={t("domainAccess.kind")} hint={t("domainAccess.permanentHint")}>
                <Select
                  size="sm"
                  value={keep ? "permanent" : "temporary"}
                  onValueChange={(value) => setPermanent(value === "permanent")}
                  items={[
                    { value: "permanent", label: t("domainAccess.permanent") },
                    { value: "temporary", label: t("domainAccess.temporary") },
                  ]}
                />
              </FormField>
            </div>
          ) : null}
        </fieldset>
        <fieldset className="flex flex-col gap-3">
          <legend className="sr-only">{t("domainAccess.password")}</legend>
          <div className="flex items-center gap-2">
            <Switch id={ids.auth} size="sm" checked={auth} onCheckedChange={setAuth} />
            <Label htmlFor={ids.auth}>{t("domainAccess.password")}</Label>
          </div>
          <p className="type-meta text-muted-foreground">
            {t("domainAccess.passwordHint")}
            {others.length > 0
              ? ` ${t("domainAccess.passwordShared", t("common.list", others))}`
              : ""}
          </p>
          {auth && !changing ? (
            <div className="flex flex-wrap items-center gap-2">
              <span className="type-meta text-muted-foreground">
                {t("domainAccess.passwordSet")}
              </span>
              <Button variant="outline" size="xs" onClick={() => setChanging(true)}>
                {t("domainAccess.change")}
              </Button>
            </div>
          ) : null}
          {wantsUser ? (
            <div className="grid gap-4 sm:grid-cols-2">
              <FormField label={t("domainAccess.username")} errors={shown(problems.username)}>
                <Input
                  size="sm"
                  value={username}
                  autoComplete="off"
                  spellCheck={false}
                  onChange={(event) => setUsername(event.target.value)}
                />
              </FormField>
              <FormField label={t("domainAccess.newPassword")} errors={shown(problems.password)}>
                <Input
                  size="sm"
                  type="password"
                  value={password}
                  autoComplete="new-password"
                  onChange={(event) => setPassword(event.target.value)}
                />
              </FormField>
            </div>
          ) : null}
        </fieldset>
        <KubeDetails
          facts={[
            [t("domainAccess.kube.annotation"), `Ingress/${route.name} ${MIDDLEWARES}`],
            [
              t("domainAccess.kube.redirect"),
              `Middleware/${redirect?.middleware ?? `${route.name}-redirect-${host.replaceAll(".", "-")}`} redirectRegex`,
            ],
            [t("domainAccess.kube.password"), `Middleware/${basicAuthName} basicAuth`],
            [t("domainAccess.kube.users"), t("domainAccess.kube.usersValue", basicAuthName)],
          ]}
        />
      </form>
    </PlanDrawer>
  )
}
