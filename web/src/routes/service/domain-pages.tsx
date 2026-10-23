import { useQuery } from "@tanstack/react-query"
import { useId, useState } from "react"

import { configMapsQuery } from "@/api/queries"
import type { Schemas } from "@/api/client"
import { FormField } from "@/components/form-field"
import { KubeDetails } from "@/components/kube-details"
import { PlanDrawer, ReviewFooter, ReviewForm } from "@/components/plan/plan-drawer"
import { usePlanStep } from "@/components/plan/planner"
import { useT } from "@/i18n"
import type { Translate } from "@/i18n"
import { useProjectSummary } from "@/lib/project-name"
import {
  defaultPage,
  ERROR_PAGE_SERVER,
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

/** The project's pages as the ConfigMap has them, or the defaults before it exists. */
function useCurrentPages(project: string) {
  const t = useT()
  const title = projectTitle(useProjectSummary(project), project)
  const query = useQuery(configMapsQuery(project))
  const found = query.data?.items.find((c) => c.origin.name === ERROR_PAGE_SERVER)
  return {
    ready: query.isSuccess,
    pages: pagesOf(found?.data),
    defaults: defaults(t, title),
  }
}

/** The form, and the changes it asks for once the pages are known. */
function usePagesForm(project: string, route: Schemas["Route"], host: string) {
  const current = useCurrentPages(project)
  const was = {
    maintenance: route.maintenance?.includes(host) ?? false,
    errorPages: route.errorPages != null,
  }
  const [form, setForm] = useState<PagesForm>({ ...was, edited: {} })
  const routeRef = { kind: "Ingress", name: route.name } as const
  const changes = current.ready
    ? pageChanges(
        routeRef,
        host,
        { ...was, pages: current.pages, defaults: current.defaults },
        form,
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
  onClose,
}: {
  project: string
  service: Service
  route: Schemas["Route"]
  host: string
  /** Traefik serves the Ingress, so it can show error pages. */
  traefik: boolean
  onClose: () => void
}) {
  const t = useT()
  const step = usePlanStep(project, onClose)
  const { current, form, setForm, changes, large } = usePagesForm(project, route, host)
  const [tried, setTried] = useState(false)
  const review = () => {
    setTried(true)
    if (large || changes.length === 0) return
    step.plan({
      title: t("domainPages.planTitle", host),
      service: { kind: service.kind, name: service.name },
      changes,
    })
  }
  const note = tried && changes.length === 0 ? t("domainPages.nothing") : undefined
  const errors = tried && large ? [t("domainPages.tooLarge")] : []
  return (
    <PlanDrawer
      step={step}
      onClose={onClose}
      title={t("domainPages.title", host)}
      description={t("domainPages.lead")}
      footer={<ReviewFooter onCancel={onClose} onReview={review} note={note} />}
    >
      <ReviewForm onReview={review}>
        <Toggle
          label={t("domainPages.maintenance")}
          hint={t("domainPages.maintenanceHint", host, service.name)}
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
        ) : null}
        <PageEditor
          pages={current.pages ?? current.defaults}
          firstUse={current.ready && !current.pages}
          edited={form.edited}
          onEdit={(key, html) => setForm({ ...form, edited: { ...form.edited, [key]: html } })}
          errors={errors}
        />
        <KubeDetails
          facts={[
            [t("domainPages.kube.server"), t("domainPages.kube.serverValue", ERROR_PAGE_SERVER)],
            [t("domainPages.kube.errors"), `Middleware/${route.name}-error-pages errors 502-504`],
            [t("domainPages.kube.maintenance"), t("domainPages.kube.maintenanceValue")],
          ]}
        />
      </ReviewForm>
    </PlanDrawer>
  )
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
  firstUse,
  edited,
  onEdit,
  errors,
}: {
  pages: Pages
  firstUse: boolean
  edited: Partial<Pages>
  onEdit: (key: PageKey, html: string) => void
  errors: string[]
}) {
  const t = useT()
  const [shown, setShown] = useState<PageKey>("maintenance")
  return (
    <fieldset className="flex flex-col gap-3">
      <legend className="mb-1.5 text-sm font-medium">{t("domainPages.pages")}</legend>
      <p className="type-meta text-muted-foreground">
        {t("domainPages.pagesHint")}
        {firstUse ? ` ${t("domainPages.firstUse")}` : ""}
      </p>
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
