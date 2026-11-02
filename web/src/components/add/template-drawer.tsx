import { useQuery } from "@tanstack/react-query"
import { useId, useState } from "react"
import ArrowLeftIcon from "~icons/ph/arrow-left"
import ArrowSquareOutIcon from "~icons/ph/arrow-square-out"
import CaretRightIcon from "~icons/ph/caret-right"

import { clusterQuery, nodesQuery, templatesQuery } from "@/api/queries"
import { FormField } from "@/components/form-field"
import { InstallAlert } from "@/components/lazy-installer-sheet"
import { PlanDrawer } from "@/components/plan/plan-drawer"
import { usePlanStep, type PlanStepState } from "@/components/plan/planner"
import { useI18n, useT, type Translate } from "@/i18n"
import { useQuantityFormat } from "@/lib/quantity"
import {
  deployProblems,
  filterTemplates,
  generateAll,
  generatedSecrets,
  listed,
  missing,
  shownInputs,
  startValues,
  inputWords,
  riskWords,
  summaryWords,
  taglineWords,
  templateChanges,
  type AppTemplate,
  type TemplateInput,
} from "@/model/app-templates"
import { suggestedHost } from "@/model/image-service"
import { templateRoom } from "@/model/template-room"
import { Alert, AlertDescription } from "@zeabur-oss/design-system/components/alert"
import { Button } from "@zeabur-oss/design-system/components/button"
import { Card } from "@zeabur-oss/design-system/components/card"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@zeabur-oss/design-system/components/collapsible"
import { Input } from "@zeabur-oss/design-system/components/input"

/** "App template": the catalogue's templates, then the form of the one picked, in one plan. */
export function TemplateDrawer({ project, onClose }: { project: string; onClose: () => void }) {
  const t = useT()
  const { locale } = useI18n()
  const list = useQuery(templatesQuery).data
  const [pickedId, setPickedId] = useState<string>()
  const picked = list?.items.find((item) => item.id === pickedId)
  const formId = useId()
  const planner = usePlanStep(project, onClose)
  const footer = (
    <div className="flex w-full flex-wrap items-center gap-2">
      {picked ? (
        <Button variant="ghost" size="sm" className="-ml-3" onClick={() => setPickedId(undefined)}>
          <ArrowLeftIcon aria-hidden="true" data-icon="inline-start" />
          {t("add.templateBack")}
        </Button>
      ) : null}
      <div className="ml-auto flex gap-2">
        <Button variant="ghost" size="sm" onClick={onClose}>
          {t("edit.cancel")}
        </Button>
        {picked ? (
          <Button size="sm" type="submit" form={formId}>
            {t("edit.review")}
          </Button>
        ) : null}
      </div>
    </div>
  )
  return (
    <PlanDrawer
      step={planner}
      onClose={onClose}
      title={picked ? t("add.templateDeploy", picked.name) : t("add.templatesTitle")}
      description={picked ? taglineWords(picked, locale) : t("add.templatesBody", project)}
      footer={footer}
    >
      {picked ? (
        <DeployForm
          key={picked.id}
          id={formId}
          project={project}
          template={picked}
          planner={planner}
        />
      ) : list ? (
        <TemplateList items={listed(list.items)} onPick={setPickedId} />
      ) : null}
    </PlanDrawer>
  )
}

/** What the template needs against what the nodes have free; nothing when either is unknown. */
function useRoom(template: AppTemplate) {
  const nodes = useQuery(nodesQuery).data
  return templateRoom(template.idle ?? template.requests, nodes)
}

function TemplateList({ items, onPick }: { items: AppTemplate[]; onPick: (id: string) => void }) {
  const t = useT()
  const { locale } = useI18n()
  const [query, setQuery] = useState("")
  const shown = filterTemplates(items, query, locale)
  return (
    <div className="flex flex-col gap-3">
      <Input
        size="sm"
        type="search"
        value={query}
        placeholder={t("add.templateFilter")}
        aria-label={t("add.templateFilter")}
        autoComplete="off"
        autoFocus
        onChange={(event) => setQuery(event.target.value)}
      />
      {shown.length === 0 ? (
        <p className="type-meta text-muted-foreground">{t("add.templateNoMatch", query.trim())}</p>
      ) : (
        <Card variant="list">
          {shown.map((template) => (
            <TemplateRow key={template.id} template={template} onPick={onPick} />
          ))}
        </Card>
      )}
    </div>
  )
}

function TemplateRow({
  template,
  onPick,
}: {
  template: AppTemplate
  onPick: (id: string) => void
}) {
  const t = useT()
  const { locale } = useI18n()
  const q = useQuantityFormat()
  const room = useRoom(template)
  const absent = missing(template)
  const tight =
    room?.verdict === "tight"
      ? t(room.resource === "cpu" ? "add.templateTightCpu" : "add.templateTightMemory")
      : undefined
  return (
    <button
      type="button"
      data-testid={`template-${template.id}`}
      className="flex w-full items-center gap-3 px-4 py-3 text-left outline-none hover:bg-v2-surface-hover focus-visible:bg-v2-surface-hover"
      onClick={() => onPick(template.id)}
    >
      <span className="flex min-w-0 flex-1 flex-col gap-0.5">
        <span className="text-sm font-medium text-main" translate="no">
          {template.name}
        </span>
        <span className="type-body text-secondary">{taglineWords(template, locale)}</span>
        <span className="type-meta text-muted-foreground">
          {t(
            "add.templateNeeds",
            q.cpu(template.requests.cpuMillicores),
            q.memory(template.requests.memoryBytes),
          )}
        </span>
        {absent.length > 0 ? (
          <span className="type-meta text-warn-strong">
            {t(
              "add.installFirst",
              t(
                "common.list",
                absent.map((r) => r.name),
              ),
            )}
          </span>
        ) : tight ? (
          <span className="type-meta text-warn-strong">{tight}</span>
        ) : null}
      </span>
      <CaretRightIcon className="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
    </button>
  )
}

function DeployForm({
  id,
  project,
  template,
  planner,
}: {
  id: string
  project: string
  template: AppTemplate
  planner: PlanStepState
}) {
  const t = useT()
  const domains = useQuery(clusterQuery).data?.domains ?? {}
  const [values, setValues] = useState(() => startValues(template))
  const [generated] = useState(() => generateAll(template))
  const [typedHost, setTypedHost] = useState<string>()
  const [tried, setTried] = useState(false)
  const host = typedHost ?? suggestedHost(template.id, project, domains.suffixes?.[0])
  const problems = deployProblems(template, values, host, domains.clusterIssuer)
  const shown = tried ? problems : undefined

  const review = () => {
    setTried(true)
    if (problems.blocked) return
    planner.plan({
      title: t("add.templateDeploy", template.name),
      changes: templateChanges(template, values, generated, host.trim()),
      notes: generatedNote(t, template),
    })
  }

  return (
    <form
      id={id}
      className="flex flex-col gap-4"
      onSubmit={(event) => {
        event.preventDefault()
        review()
      }}
    >
      <About template={template} />
      <Requirements template={template} issuer={domains.clusterIssuer} />
      <RoomNote template={template} />
      <Risks template={template} />
      {template.needsDomain ? (
        <DomainField
          host={host}
          domains={domains}
          problem={shown?.domain}
          onChange={setTypedHost}
        />
      ) : null}
      {shownInputs(template).map((input) => (
        <InputField
          key={input.name}
          input={input}
          value={values[input.name] ?? ""}
          problem={shown?.inputs[input.name]}
          onChange={(value) => setValues((v) => ({ ...v, [input.name]: value }))}
        />
      ))}
      {generatedNote(t, template).map((note) => (
        <p key={note} className="type-meta text-muted-foreground">
          {note}
        </p>
      ))}
    </form>
  )
}

/** Where the generated values go, when the template has any. */
function generatedNote(t: Translate, template: AppTemplate) {
  const secrets = generatedSecrets(template)
  return secrets.length > 0 ? [t("add.templateGenerated", t("common.list", secrets))] : []
}

/** The components the cluster lacks, a missing issuer, and the components Zeabur cannot see. */
function Requirements({ template, issuer }: { template: AppTemplate; issuer?: string | null }) {
  const t = useT()
  const unknown = template.requires.filter((r) => r.state === "unknown")
  return (
    <>
      {missing(template).length > 0 ? <Missing template={template} /> : null}
      {template.needsClusterIssuer && !issuer ? (
        <Alert variant="warn">
          <AlertDescription>{t("add.templateNoIssuer")}</AlertDescription>
        </Alert>
      ) : null}
      {unknown.length > 0 ? (
        <p className="type-meta text-muted-foreground">
          {t(
            "add.templateUnknown",
            t(
              "common.list",
              unknown.map((r) => r.name),
            ),
          )}
        </p>
      ) : null}
    </>
  )
}

function DomainField({
  host,
  domains,
  problem,
  onChange,
}: {
  host: string
  domains: { suffixes?: string[] | null; clusterIssuer?: string | null }
  problem?: "empty" | "invalid"
  onChange: (host: string) => void
}) {
  const t = useT()
  const suffixed = !!domains.suffixes?.length
  return (
    <FormField
      label={t("add.publicUrl")}
      hint={
        <>
          {suffixed ? null : <span className="block">{t("add.hostHint")}</span>}
          {domains.clusterIssuer ? (
            <span className="block">{t("add.https", domains.clusterIssuer)}</span>
          ) : null}
        </>
      }
      errors={problem ? [t(`add.problem.host.${problem}`)] : []}
    >
      <Input
        size="sm"
        mono
        value={host}
        placeholder={suffixed ? undefined : "app.example.com"}
        autoComplete="off"
        spellCheck={false}
        onChange={(event) => onChange(event.target.value.trim())}
      />
    </FormField>
  )
}

function About({ template }: { template: AppTemplate }) {
  const t = useT()
  return (
    <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
      <span className="type-meta text-muted-foreground" translate="no">
        {template.license}
      </span>
      <a
        href={template.homepage}
        target="_blank"
        rel="noreferrer noopener"
        className="inline-flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground"
      >
        {t("settings.homepage")}
        <ArrowSquareOutIcon className="size-3" aria-hidden="true" />
        <span className="sr-only">{t("settings.opensInNewTab")}</span>
      </a>
    </div>
  )
}

function Missing({ template }: { template: AppTemplate }) {
  const t = useT()
  const absent = missing(template).map((r) => ({
    id: r.component ? `${r.capability}/${r.component}` : r.capability,
    name: r.name,
  }))
  return (
    <InstallAlert
      variant="warn"
      title={t(
        "add.installFirst",
        t(
          "common.list",
          absent.map((r) => r.name),
        ),
      )}
      text={t("add.templateMissingBody", absent.length > 1)}
      components={absent}
    />
  )
}

/** What the template's author says to know before deploying it; the conclusion, then the notes. */
function Risks({ template }: { template: AppTemplate }) {
  const t = useT()
  const { locale } = useI18n()
  if (template.risks.length === 0) return null
  const summary = summaryWords(template, locale)
  return (
    <Collapsible>
      <CollapsibleTrigger variant="disclosure">
        <CaretRightIcon aria-hidden="true" />
        {t("add.templateRisks", template.risks.length)}
      </CollapsibleTrigger>
      <CollapsibleContent>
        <p className="pt-1 type-meta text-muted-foreground">{t("add.templateRisksBody")}</p>
        {summary ? (
          <p className="pt-2 type-body text-main" lang={summary.english ? "en" : undefined}>
            {summary.text}
          </p>
        ) : null}
        <ul className="flex flex-col gap-3 pt-2">
          {template.risks.map((r) => {
            const words = riskWords(r, locale)
            return (
              <li
                key={r.risk}
                className="flex flex-col gap-0.5"
                lang={words.english ? "en" : undefined}
                translate={words.english ? "no" : undefined}
              >
                <span className="type-body text-main">{words.risk}</span>
                <span className="type-meta text-secondary">
                  <span className="font-medium">{t("add.templateRiskFix")}</span> {words.mitigation}
                </span>
              </li>
            )
          })}
        </ul>
      </CollapsibleContent>
    </Collapsible>
  )
}

function RoomNote({ template }: { template: AppTemplate }) {
  const t = useT()
  const q = useQuantityFormat()
  const room = useRoom(template)
  if (!room) return null
  const [use, free] =
    room.resource === "cpu"
      ? [q.cpu(room.use), q.cpu(room.free)]
      : [q.memory(room.use), q.memory(room.free)]
  if (room.verdict === "room")
    return <p className="type-meta text-muted-foreground">{t("add.templateRoom", use, free)}</p>
  return (
    <Alert variant="warn">
      <AlertDescription>
        {t(room.resource === "cpu" ? "add.templateLowCpu" : "add.templateLowMemory", use, free)}
      </AlertDescription>
    </Alert>
  )
}

function InputField({
  input,
  value,
  problem,
  onChange,
}: {
  input: TemplateInput
  value: string
  problem?: "empty" | "invalid"
  onChange: (value: string) => void
}) {
  const t = useT()
  const { locale } = useI18n()
  const words = inputWords(input, locale)
  const label = (
    <span className="flex items-center gap-2">
      <span>{words.label}</span>
      {input.required ? null : (
        <span className="font-normal text-muted-foreground">{t("add.templateOptional")}</span>
      )}
    </span>
  )
  return (
    <FormField
      label={label}
      hint={
        input.default
          ? `${words.description} ${t("add.templateInputDefault", input.default)}`
          : words.description
      }
      errors={
        problem === "empty"
          ? [t("add.templateInputEmpty")]
          : problem === "invalid"
            ? [t("add.templateInputInvalid", input.pattern)]
            : []
      }
    >
      <Input
        size="sm"
        mono
        value={value}
        placeholder={input.default ?? undefined}
        autoComplete="off"
        spellCheck={false}
        onChange={(event) => onChange(event.target.value.trim())}
      />
    </FormField>
  )
}
