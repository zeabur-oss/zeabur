// A project's settings: its details (display name, description, icon; annotations on its
// namespace) and whether the namespace shows as a project. Both are plans like any change. Who
// may not patch the namespace sees the details read-only, with the right to ask for, or, while
// admin rights are offered, the way to use them (the hint is checked with everyday rights); the
// permission is the API's hint (`canChange`), and when it is unknown the form stays open and the
// plan's dry run decides.
import { useForm } from "@tanstack/react-form"
import { useQuery } from "@tanstack/react-query"
import { getRouteApi } from "@tanstack/react-router"
import { useState, type ReactNode } from "react"

import type { Schemas } from "@/api/client"
import { namespaceQuery } from "@/api/queries"
import { AccessDialog, AdoptButton } from "@/components/adopt"
import { FormField } from "@/components/form-field"
import { LoadingRows } from "@/components/page"
import { usePlanner } from "@/components/plan/planner"
import { Problem } from "@/components/problem"
import { permission, RefusedHint, useHintOffersElevation } from "@/components/refused"
import { projectGlyphs } from "@/components/project-icon"
import { Button } from "@zeabur/design-system/components/button"
import { Input } from "@zeabur/design-system/components/input"
import { Textarea } from "@zeabur/design-system/components/textarea"
import { ToggleGroup, ToggleGroupItem } from "@zeabur/design-system/components/toggle-group"
import { useT } from "@/i18n"
import { mayChange, projectIcons, type ProjectIconName } from "@/model/project"
import { ProjectHeader } from "@/routes/project/header"

const route = getRouteApi("/projects/$project/settings")

/** The longest display name and description (zeabur-model `DISPLAY_NAME_MAX`, `DESCRIPTION_MAX`). */
export const DISPLAY_NAME_MAX = 64
export const DESCRIPTION_MAX = 500

export function ProjectSettingsPage() {
  const { project } = route.useParams()
  const t = useT()
  const namespace = useQuery(namespaceQuery(project))
  const data = namespace.data
  return (
    <>
      <ProjectHeader project={project} entry="settings" />
      <div className="flex max-w-3xl flex-col gap-8 px-4 py-5 pb-16 md:px-8">
        {namespace.isPending ? (
          <LoadingRows label={t("common.loading")} rows={4} />
        ) : namespace.isError ? (
          <Problem error={namespace.error} />
        ) : data ? (
          <ProjectSettings namespace={data} />
        ) : null}
      </div>
    </>
  )
}

export function ProjectSettings({ namespace }: { namespace: Schemas["ProjectNamespace"] }) {
  const t = useT()
  const [help, setHelp] = useState(false)
  const readOnly = !mayChange(namespace)
  const elevation = useHintOffersElevation(namespace.canChange)
  return (
    <>
      {namespace.adopted ? (
        <Section title={t("projectSettings.details")}>
          {elevation ? (
            <RefusedHint
              hint={namespace.canChange}
              what={t("projectSettings.changeDetails")}
              missing={[permission("patch", "namespaces")]}
            />
          ) : readOnly ? (
            <p className="flex flex-wrap items-center gap-2 text-sm text-muted-foreground">
              {t("projectSettings.readOnly")}
              <Button size="sm" variant="link" onClick={() => setHelp(true)}>
                {t("projectSettings.getAccess")}
              </Button>
            </p>
          ) : null}
          <DetailsForm
            // A new form when the saved details change, so it starts from them.
            key={JSON.stringify([namespace.displayName, namespace.description, namespace.icon])}
            namespace={namespace}
            readOnly={readOnly}
          />
        </Section>
      ) : null}
      <Section title={t("projectSettings.project")}>
        {namespace.system ? (
          <p className="text-sm text-muted-foreground">{t("projectSettings.system")}</p>
        ) : (
          <div className="flex flex-wrap items-center justify-between gap-3 rounded-lg border px-4 py-3">
            <div className="flex min-w-0 flex-col gap-0.5 text-sm">
              <span>
                {namespace.adopted
                  ? t("projectSettings.adopted", namespace.name)
                  : t("projectSettings.notAdopted", namespace.name)}
              </span>
            </div>
            <AdoptButton
              namespace={namespace.name}
              adopted={namespace.adopted}
              variant={namespace.adopted ? "destructive" : "outline"}
            />
          </div>
        )}
      </Section>
      {help ? (
        <AccessDialog
          namespace={namespace.name}
          adopted={namespace.adopted}
          onClose={() => setHelp(false)}
          what={t("projectSettings.changeDetails")}
        />
      ) : null}
    </>
  )
}

function DetailsForm({
  namespace,
  readOnly,
}: {
  namespace: Schemas["ProjectNamespace"]
  readOnly: boolean
}) {
  const t = useT()
  const planner = usePlanner(namespace.name)
  const initial = {
    displayName: namespace.displayName ?? "",
    description: namespace.description ?? "",
    icon: (namespace.icon ?? undefined) as ProjectIconName | undefined,
  }
  const form = useForm({
    defaultValues: initial,
    onSubmit: ({ value }) =>
      planner.plan({
        title: t("title.details", namespace.name),
        changes: [
          {
            op: "details",
            displayName: value.displayName.trim() || undefined,
            description: value.description.trim() || undefined,
            icon: value.icon,
          },
        ],
      }),
  })
  const atMost =
    (max: number) =>
    ({ value }: { value: string }) =>
      [...value.trim()].length > max ? t("projectSettings.tooLong", max) : undefined
  return (
    <form
      className="flex flex-col gap-4"
      onSubmit={(event) => {
        event.preventDefault()
        if (!readOnly) void form.handleSubmit()
      }}
    >
      <form.Field name="displayName" validators={{ onChange: atMost(DISPLAY_NAME_MAX) }}>
        {(f) => (
          <FormField label={t("projectSettings.displayName")} errors={f.state.meta.errors}>
            <Input
              value={f.state.value}
              placeholder={namespace.name}
              disabled={readOnly}
              onChange={(event) => f.handleChange(event.target.value)}
              onBlur={f.handleBlur}
            />
          </FormField>
        )}
      </form.Field>
      <form.Field name="description" validators={{ onChange: atMost(DESCRIPTION_MAX) }}>
        {(f) => (
          <FormField label={t("projectSettings.description")} errors={f.state.meta.errors}>
            <Textarea
              value={f.state.value}
              rows={3}
              disabled={readOnly}
              onChange={(event) => f.handleChange(event.target.value)}
              onBlur={f.handleBlur}
            />
          </FormField>
        )}
      </form.Field>
      <form.Field name="icon">
        {(f) => (
          <div className="flex flex-col gap-1">
            <span className="type-meta text-muted-foreground">{t("projectSettings.icon")}</span>
            <ToggleGroup
              aria-label={t("projectSettings.icon")}
              value={f.state.value ? [f.state.value] : []}
              onValueChange={(next) => {
                const choice = next[0]
                f.handleChange(projectIcons.find((name) => name === choice))
              }}
              disabled={readOnly}
              variant="outline"
              className="flex-wrap"
            >
              {projectIcons.map((name) => {
                const Glyph = projectGlyphs[name]
                return (
                  <ToggleGroupItem
                    key={name}
                    value={name}
                    aria-label={t(`icon.${name}`)}
                    title={t(`icon.${name}`)}
                    variant="outline"
                  >
                    <Glyph aria-hidden="true" />
                  </ToggleGroupItem>
                )
              })}
            </ToggleGroup>
          </div>
        )}
      </form.Field>
      {readOnly ? null : (
        <form.Subscribe
          selector={(s) =>
            [
              s.canSubmit,
              s.values.displayName.trim() !== initial.displayName ||
                s.values.description.trim() !== initial.description ||
                s.values.icon !== initial.icon,
            ] as const
          }
        >
          {([canSubmit, changed]) => (
            <div>
              <Button type="submit" size="sm" disabled={!changed || !canSubmit}>
                {t("action.saveDetails")}
              </Button>
            </div>
          )}
        </form.Subscribe>
      )}
      {planner.dialog}
    </form>
  )
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-3">
      <h2 className="text-sm font-medium">{title}</h2>
      {children}
    </section>
  )
}
