// Deleting the namespace is not offered: plans refuse it (`tenant_level`).
import { useForm } from "@tanstack/react-form"
import { useQuery } from "@tanstack/react-query"
import { getRouteApi } from "@tanstack/react-router"
import { useState, type ReactNode } from "react"
import CaretDownIcon from "~icons/ph/caret-down"

import type { Schemas } from "@/api/client"
import { namespaceQuery, projectQuery } from "@/api/queries"
import { AccessDialog, AdoptButton } from "@/components/adopt"
import { CopyButton } from "@/components/bits"
import { FormField } from "@/components/form-field"
import { LoadingRows } from "@/components/page"
import { usePlanner } from "@/components/plan/planner"
import { Problem } from "@/components/problem"
import { projectGlyphs } from "@/components/project-icon"
import { permission, RefusedHint, useHintOffersElevation } from "@/components/refused"
import { useT } from "@/i18n"
import { mayChange, projectIcons, type ProjectIconName } from "@/model/project"
import type { Service } from "@/model/service"
import { Button } from "@zeabur/design-system/components/button"
import { Card, CardFooter, CardRow, SectionLabel } from "@zeabur/design-system/components/card"
import { Input } from "@zeabur/design-system/components/input"
import { PageHeader } from "@zeabur/design-system/components/page-header"
import { Popover, PopoverContent, PopoverTrigger } from "@zeabur/design-system/components/popover"
import { Textarea } from "@zeabur/design-system/components/textarea"
import { ToggleGroup, ToggleGroupItem } from "@zeabur/design-system/components/toggle-group"
import { PageColumn } from "@zeabur/design-system/components/page-column"

const route = getRouteApi("/projects/$project/settings")

/** zeabur-model `DISPLAY_NAME_MAX`, `DESCRIPTION_MAX`. */
export const DISPLAY_NAME_MAX = 64
export const DESCRIPTION_MAX = 500

export function ProjectSettingsPage() {
  const { project } = route.useParams()
  const t = useT()
  const namespace = useQuery(namespaceQuery(project))
  const data = namespace.data
  return (
    <PageColumn width="content" gap="xl" className="mt-6 mb-20 md:mt-8">
      <PageHeader title={t("projectSettings.general")} />
      {namespace.isPending ? (
        <LoadingRows label={t("common.loading")} rows={4} />
      ) : namespace.isError ? (
        <Problem error={namespace.error} />
      ) : data ? (
        <ProjectSettings namespace={data} />
      ) : null}
    </PageColumn>
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
        <Group label={t("projectSettings.basic")}>
          {elevation ? (
            <div className="mb-3">
              <RefusedHint
                hint={namespace.canChange}
                what={t("projectSettings.changeDetails")}
                missing={[permission("patch", "namespaces")]}
              />
            </div>
          ) : readOnly ? (
            <p className="mb-3 flex flex-wrap items-center gap-2 text-sm text-secondary">
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
        </Group>
      ) : null}
      <Group label={t("projectSettings.namespaceGroup")}>
        <Card variant="list">
          <CardRow
            label={t("projectSettings.namespace")}
            description={t("projectSettings.namespaceHint")}
          >
            <span className="flex items-center gap-1 font-mono text-sm" translate="no">
              {namespace.name}
              <CopyButton value={namespace.name} />
            </span>
          </CardRow>
          <CardRow
            label={t("projectSettings.shown")}
            description={
              namespace.system
                ? t("projectSettings.system")
                : namespace.adopted
                  ? t("projectSettings.shownYes")
                  : t("projectSettings.shownNo")
            }
          >
            {namespace.system || namespace.adopted ? null : (
              <AdoptButton namespace={namespace.name} adopted={false} />
            )}
          </CardRow>
        </Card>
      </Group>
      {namespace.adopted ? <BatchActions project={namespace.name} /> : null}
      {namespace.adopted && !namespace.system ? (
        <Group label={t("projectSettings.danger")}>
          <Card variant="list">
            <CardRow
              label={t("projectSettings.remove")}
              description={t("projectSettings.removeHint")}
            >
              <AdoptButton
                namespace={namespace.name}
                adopted
                variant="destructive"
                label={t("projectSettings.removeAction")}
              />
            </CardRow>
          </Card>
        </Group>
      ) : null}
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

function Group({ label, children }: { label: string; children: ReactNode }) {
  return (
    <section>
      <SectionLabel>{label}</SectionLabel>
      {children}
    </section>
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
      onSubmit={(event) => {
        event.preventDefault()
        if (!readOnly) void form.handleSubmit()
      }}
    >
      <Card variant="list">
        <form.Field name="icon">
          {(f) => (
            <CardRow label={t("projectSettings.icon")} description={t("projectSettings.iconHint")}>
              <IconPicker value={f.state.value} onChange={f.handleChange} disabled={readOnly} />
            </CardRow>
          )}
        </form.Field>
        <form.Field name="displayName" validators={{ onChange: atMost(DISPLAY_NAME_MAX) }}>
          {(f) => (
            <CardRow
              label={t("projectSettings.name")}
              description={t("projectSettings.nameHint", namespace.name)}
            >
              <FormField
                label={t("projectSettings.name")}
                hideLabel
                errors={f.state.meta.errors}
                className="w-full sm:w-70"
              >
                <Input
                  value={f.state.value}
                  placeholder={namespace.name}
                  disabled={readOnly}
                  onChange={(event) => f.handleChange(event.target.value)}
                  onBlur={f.handleBlur}
                />
              </FormField>
            </CardRow>
          )}
        </form.Field>
        <form.Field name="description" validators={{ onChange: atMost(DESCRIPTION_MAX) }}>
          {(f) => (
            <CardRow
              label={t("projectSettings.description")}
              description={t("projectSettings.descriptionHint")}
              align="start"
            >
              <FormField
                label={t("projectSettings.description")}
                hideLabel
                errors={f.state.meta.errors}
                className="w-full sm:w-80"
              >
                <Textarea
                  value={f.state.value}
                  rows={3}
                  disabled={readOnly}
                  onChange={(event) => f.handleChange(event.target.value)}
                  onBlur={f.handleBlur}
                />
              </FormField>
            </CardRow>
          )}
        </form.Field>
        {readOnly ? null : (
          <form.Subscribe
            selector={(s) =>
              [
                s.values.displayName.trim() !== initial.displayName ||
                  s.values.description.trim() !== initial.description ||
                  s.values.icon !== initial.icon,
              ] as const
            }
          >
            {([changed]) =>
              changed ? (
                <CardFooter className="justify-end">
                  <Button type="button" variant="ghost" size="sm" onClick={() => form.reset()}>
                    {t("draft.discard")}
                  </Button>
                  <Button type="submit" size="sm">
                    {t("action.saveDetails")}
                  </Button>
                </CardFooter>
              ) : null
            }
          </form.Subscribe>
        )}
      </Card>
      {planner.dialog}
    </form>
  )
}

function IconPicker({
  value,
  onChange,
  disabled,
}: {
  value?: ProjectIconName
  onChange: (icon: ProjectIconName | undefined) => void
  disabled: boolean
}) {
  const t = useT()
  const Current = projectGlyphs[value ?? "folder"]
  return (
    <span className="flex items-center gap-3">
      <span className="flex size-9 items-center justify-center rounded-lg border border-main bg-secondary text-main">
        <Current aria-hidden="true" className="size-4.5" />
      </span>
      <Popover>
        <PopoverTrigger
          render={<Button type="button" variant="outline" size="sm" disabled={disabled} />}
        >
          {t("projectSettings.changeIcon")}
          <CaretDownIcon aria-hidden="true" data-icon="inline-end" />
        </PopoverTrigger>
        <PopoverContent align="end" className="w-64">
          <ToggleGroup
            aria-label={t("projectSettings.icon")}
            value={value ? [value] : []}
            onValueChange={(next) => {
              const choice = next[0]
              onChange(projectIcons.find((name) => name === choice))
            }}
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
        </PopoverContent>
      </Popover>
    </span>
  )
}

const restartable = (s: Service) =>
  s.kind === "Deployment" || s.kind === "StatefulSet" || s.kind === "DaemonSet"

/** Pausing skips what another tool manages: it would scale it back. */
function BatchActions({ project }: { project: string }) {
  const t = useT()
  const view = useQuery(projectQuery(project))
  const planner = usePlanner(project)
  const services = view.data?.services ?? []
  const running = services.filter(
    (s) => (s.kind === "Deployment" || s.kind === "StatefulSet") && (s.replicas?.desired ?? 0) > 0,
  )
  const pausable = running.filter((s) => !s.provenance.tool)
  const skipped = running.flatMap((s) =>
    s.provenance.tool ? [{ name: s.name, tool: s.provenance.tool }] : [],
  )
  const restarting = services.filter(restartable)
  if (services.length === 0) return null
  return (
    <Group label={t("projectSettings.batch")}>
      <Card variant="list">
        <CardRow
          label={t("projectSettings.pauseAll")}
          description={t("projectSettings.pauseAllHint")}
        >
          <Button
            variant="outline"
            size="sm"
            disabled={pausable.length === 0}
            onClick={() =>
              planner.plan({
                title: t("title.pauseAll", project),
                skipped,
                changes: pausable.map((s) => ({
                  op: "scale" as const,
                  workload: { kind: s.kind, name: s.name },
                  replicas: 0,
                })),
              })
            }
          >
            {t("projectSettings.pause")}
          </Button>
        </CardRow>
        <CardRow
          label={t("projectSettings.restartAll")}
          description={t("projectSettings.restartAllHint")}
        >
          <Button
            variant="outline"
            size="sm"
            disabled={restarting.length === 0}
            onClick={() =>
              planner.plan({
                title: t("title.restartAll", project),
                changes: restarting.map((s) => ({
                  op: "restart" as const,
                  workload: { kind: s.kind, name: s.name },
                })),
              })
            }
          >
            {t("ops.restart")}
          </Button>
        </CardRow>
      </Card>
      {planner.dialog}
    </Group>
  )
}
