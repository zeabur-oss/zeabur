import { useQuery } from "@tanstack/react-query"
import { useId, useState } from "react"

import { configMapsQuery, projectQuery } from "@/api/queries"
import type { Schemas } from "@/api/client"
import { FormField } from "@/components/form-field"
import { KubeDetails } from "@/components/kube-details"
import { ReviewWord } from "@/components/plan/draft"
import { FormDrawer, ReviewFooter, ReviewForm } from "@/components/plan/plan-drawer"
import type { PlanAsk } from "@/components/plan/planner"
import { useT } from "@/i18n"
import type { Translate } from "@/i18n"
import { useProjectSummary } from "@/lib/project-name"
import {
  defaultPage,
  PAGE_KEYS,
  pageChanges,
  pagesOf,
  tooLarge,
  type PageKey,
  type Pages,
  type PagesForm,
} from "@/model/error-pages"
import { projectTitle } from "@/model/project"
import type { Service } from "@/model/service"
import { Alert, AlertDescription, AlertTitle } from "@zeabur/design-system/components/alert"
import { Label } from "@zeabur/design-system/components/label"
import { Select } from "@zeabur/design-system/components/select"
import { Switch } from "@zeabur/design-system/components/switch"
import { Textarea } from "@zeabur/design-system/components/textarea"

function defaults(t: Translate, project: string): Pages {
  const page = (heading: string, line: string) =>
    defaultPage(t("domainPages.default.lang"), project, heading, line)
  return {
    maintenance: page(
      t("domainPages.default.maintenance", project),
      t("domainPages.default.later"),
    ),
    badGateway: page(t("domainPages.default.badGateway", project), t("domainPages.default.retry")),
    serviceUnavailable: page(
      t("domainPages.default.serviceUnavailable", project),
      t("domainPages.default.retry"),
    ),
    gatewayTimeout: page(
      t("domainPages.default.gatewayTimeout", project),
      t("domainPages.default.retry"),
    ),
  }
}

/** The project's page server and its pages, or the defaults before it exists. */
function useCurrentPages(project: string) {
  const t = useT()
  const title = projectTitle(useProjectSummary(project), project)
  const view = useQuery(projectQuery(project))
  const query = useQuery(configMapsQuery(project))
  const server = view.data?.pageServer ?? undefined
  const found = query.data?.items.find((c) => c.origin.name === server?.pages)
  return {
    ready: view.isSuccess && query.isSuccess,
    server: server?.service,
    pages: pagesOf(found?.data),
    defaults: defaults(t, title),
  }
}

/** The form, and the changes it asks for once the pages are known. */
function usePagesForm(
  project: string,
  route: Schemas["Route"],
  host: string,
  workload: Schemas["WorkloadRef"],
) {
  const current = useCurrentPages(project)
  const was = {
    maintenance: route.maintenance?.includes(host) ?? false,
    errorPages: route.errorPages != null,
  }
  const [form, setForm] = useState<PagesForm>({ ...was, edited: {} })
  const routeRef = { kind: route.origin.kind as Schemas["RouteKind"], name: route.name }
  const changes = current.ready
    ? pageChanges(
        routeRef,
        host,
        {
          ...was,
          server: current.server !== undefined,
          pages: current.pages,
          defaults: current.defaults,
        },
        form,
        workload,
      )
    : []
  return {
    current,
    form,
    setForm,
    changes,
    large: PAGE_KEYS.some((key) => tooLarge(form.edited[key] ?? "")),
  }
}

/** Maintenance mode for one domain, and the project's error pages on its Ingress. */
export default function DomainPagesDrawer({
  project,
  service,
  route,
  host,
  traefik,
  plan,
  onClose,
}: {
  project: string
  service: Service
  route: Schemas["Route"]
  host: string
  /** Traefik serves the Ingress, so it can show error pages. An HTTPRoute can't. */
  traefik: boolean
  plan: (ask: PlanAsk) => boolean
  onClose: () => void
}) {
  const t = useT()
  const { current, form, setForm, changes, large } = usePagesForm(project, route, host, {
    kind: service.kind,
    name: service.name,
  })
  const [tried, setTried] = useState(false)
  const review = () => {
    setTried(true)
    if (large || changes.length === 0) return
    plan({
      title: t("domainPages.planTitle", host),
      service: { kind: service.kind, name: service.name },
      changes,
    })
    onClose()
  }
  const note = tried && changes.length === 0 ? t("domainPages.nothing") : undefined
  const errors = tried && large ? [t("domainPages.tooLarge")] : []
  return (
    <FormDrawer
      onClose={onClose}
      title={t("domainPages.title", host)}
      description={t("domainPages.lead")}
      footer={
        <ReviewFooter onCancel={onClose} onReview={review} note={note} label={<ReviewWord />} />
      }
    >
      <ReviewForm onReview={review}>
        <NoServer shown={current.ready && current.server === undefined} />
        <Toggle
          label={t("domainPages.maintenance")}
          hint={maintenanceHint(t, service, route, host)}
          checked={form.maintenance}
          onChange={(maintenance) => setForm({ ...form, maintenance })}
        />
        {traefik ? (
          <Toggle
            label={t("domainPages.errorPages")}
            hint={errorPagesHint(t, service, route, host)}
            checked={form.errorPages}
            onChange={(errorPages) => setForm({ ...form, errorPages })}
          />
        ) : route.origin.kind === "HTTPRoute" ? (
          <p className="type-meta text-muted-foreground">{t("domainPages.gatewayNoErrorPages")}</p>
        ) : null}
        <PageEditor
          pages={current.pages ?? current.defaults}
          edited={form.edited}
          onEdit={(key, html) => setForm({ ...form, edited: { ...form.edited, [key]: html } })}
          errors={errors}
        />
        <KubeDetails facts={pagesFacts(t, route, current.server)} />
      </ReviewForm>
    </FormDrawer>
  )
}

/** Says that review also deploys the Error pages template. */
function NoServer({ shown }: { shown: boolean }) {
  const t = useT()
  if (!shown) return null
  return (
    <Alert variant="info">
      <AlertTitle>{t("domainPages.noServer")}</AlertTitle>
      <AlertDescription>{t("domainPages.noServerHint")}</AlertDescription>
    </Alert>
  )
}

function pagesFacts(t: Translate, route: Schemas["Route"], name?: string): [string, string][] {
  const server: [string, string] = [
    t("domainPages.kube.server"),
    name === undefined ? t("domainPages.kube.serverNew") : t("domainPages.kube.serverValue", name),
  ]
  const maintenance: [string, string] = [
    t("domainPages.kube.maintenance"),
    t("domainPages.kube.maintenanceValue"),
  ]
  if (route.origin.kind === "HTTPRoute") return [server, maintenance]
  const errors: [string, string] = [
    t("domainPages.kube.errors"),
    `Middleware/${route.name}-error-pages errors 502-504`,
  ]
  return [server, errors, maintenance]
}

function maintenanceHint(t: Translate, service: Service, route: Schemas["Route"], host: string) {
  const hint = t("domainPages.maintenanceHint", host, service.name)
  const others = route.hostnames.filter((h) => h !== host)
  if (route.origin.kind !== "HTTPRoute" || others.length === 0) return hint
  return `${hint} ${t("domainPages.gatewayMoves", t("common.list", others))}`
}

function errorPagesHint(t: Translate, service: Service, route: Schemas["Route"], host: string) {
  const others = route.hostnames.filter((h) => h !== host)
  const shared = others.length > 0 ? ` ${t("domainPages.shared", t("common.list", others))}` : ""
  return `${t("domainPages.errorPagesHint", service.name)}${shared}`
}

function Toggle({
  label,
  hint,
  checked,
  onChange,
}: {
  label: string
  hint: string
  checked: boolean
  onChange: (checked: boolean) => void
}) {
  const id = useId()
  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex items-center gap-2">
        <Switch id={id} size="sm" checked={checked} onCheckedChange={onChange} />
        <Label htmlFor={id}>{label}</Label>
      </div>
      <p className="type-meta text-muted-foreground">{hint}</p>
    </div>
  )
}

function PageEditor({
  pages,
  edited,
  onEdit,
  errors,
}: {
  pages: Pages
  edited: Partial<Pages>
  onEdit: (key: PageKey, html: string) => void
  errors: string[]
}) {
  const t = useT()
  const [shown, setShown] = useState<PageKey>("maintenance")
  return (
    <fieldset className="flex flex-col gap-3">
      <legend className="mb-1.5 text-sm font-medium">{t("domainPages.pages")}</legend>
      <p className="type-meta text-muted-foreground">{t("domainPages.pagesHint")}</p>
      <FormField label={t("domainPages.page")}>
        <Select
          size="sm"
          value={shown}
          onValueChange={(value) => setShown(value as PageKey)}
          items={PAGE_KEYS.map((key) => ({ value: key, label: t(`domainPages.page.${key}`) }))}
        />
      </FormField>
      <FormField label={t("domainPages.html")} errors={errors}>
        <Textarea
          mono
          className="min-h-48 max-h-96"
          value={edited[shown] ?? pages[shown]}
          onChange={(event) => onEdit(shown, event.target.value)}
        />
      </FormField>
    </fieldset>
  )
}
