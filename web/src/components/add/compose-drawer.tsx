import { useMutation, useQuery } from "@tanstack/react-query"
import { useNavigate } from "@tanstack/react-router"
import { useId, useState, type ReactNode } from "react"

import { api, toProblem, unwrap } from "@/api/client"
import {
  clusterQuery,
  creatableQuery,
  databasesQuery,
  nodesQuery,
  objectStorageQuery,
} from "@/api/queries"
import { useTakenNames } from "@/components/add/taken"
import { useGates } from "@/components/gate"
import { PlanDrawer } from "@/components/plan/plan-drawer"
import { usePlanStep } from "@/components/plan/planner"
import { useProjectSecurity } from "@/components/security-profile"
import { TextFile } from "@/components/text-file"
import { useT, type Translate } from "@/i18n"
import {
  composeChanges,
  imageServices,
  managedOffer,
  portsOf,
  type ComposeChoices,
  type ComposeService,
  type Conversion,
  type KeptReason,
  type Managed,
} from "@/model/compose"
import { engineName } from "@/model/databases"
import {
  hostProblem,
  imageOptions,
  rightsOf,
  serviceNameProblem,
  suggestedHost,
} from "@/model/image-service"
import { Alert, AlertDescription } from "@zeabur/design-system/components/alert"
import { Button } from "@zeabur/design-system/components/button"
import { Card, CardRow, SectionLabel } from "@zeabur/design-system/components/card"
import { Input } from "@zeabur/design-system/components/input"
import { Label } from "@zeabur/design-system/components/label"
import { Switch } from "@zeabur/design-system/components/switch"

type Finding = ComposeService["findings"][number]
type Offer = ReturnType<typeof managedOffer>
type Files = Record<string, string>

/** What the project allows and has, for turning the file into objects. */
function useComposeContext(project: string) {
  const domains = useQuery(clusterQuery).data?.domains ?? {}
  const options = imageOptions(rightsOf(useQuery(creatableQuery(project)).data))
  const operators = useQuery(databasesQuery(project)).data?.operators ?? []
  const store = useQuery(objectStorageQuery(project)).data?.state
  const nodes = useQuery(nodesQuery).data?.items.length || 1
  const gates = useGates(project)
  const databases = gates("changeDatabases").open
  const objectStorage = gates("changeObjectStorage").open
  const mayCreate = (engine: Managed) => (engine === "objectStorage" ? objectStorage : databases)
  const { enforced } = useProjectSecurity(project)
  const taken = useTakenNames(project)
  const states: Partial<Record<Managed, (typeof operators)[number]["state"]>> = {}
  for (const o of operators) states[o.engine] = o.state
  if (store) states.objectStorage = store
  return { domains, options, nodes, enforced, states, mayCreate, taken }
}

/** The file as the server reads it, and whether the text changed since. */
function useConversion(project: string, yaml: string, envFiles: Files) {
  const [read, setRead] = useState<{ conversion: Conversion; source: string }>()
  const source = JSON.stringify([yaml, envFiles])
  const convert = useMutation({
    mutationFn: async () =>
      unwrap(
        await api.POST("/api/v1/projects/{project}/compose", {
          params: { path: { project } },
          body: { yaml, envFiles },
        }),
      ),
    onSuccess: (conversion) => setRead({ conversion, source }),
  })
  return { convert, conversion: read?.conversion, stale: !!read && read.source !== source }
}

/** What each service offers, and what the person chose for it. */
function useChoices(
  conversion: Conversion | undefined,
  { states, mayCreate }: ReturnType<typeof useComposeContext>,
) {
  const [unmanaged, setUnmanaged] = useState<ReadonlySet<string>>(new Set())
  const [published, setPublished] = useState<ReadonlyMap<string, string>>(new Map())
  const services = conversion?.services ?? []
  const offers = new Map(services.map((s) => [s.name, managedOffer(s, states, mayCreate)]))
  const managed = services
    .filter((s) => offersEngine(offers.get(s.name)) && !unmanaged.has(s.name))
    .map((s) => s.name)
  const choices: ComposeChoices = { managed: new Set(managed), published }
  return { offers, choices, setUnmanaged, setPublished }
}

/** "Docker Compose": the server reads the file; each service is planned as the "Docker image"
 * form plans one, and each database image as the database form, in one plan. */
export function ComposeDrawer({ project, onClose }: { project: string; onClose: () => void }) {
  const t = useT()
  const navigate = useNavigate()
  const context = useComposeContext(project)
  const [yaml, setYaml] = useState("")
  const [envFiles, setEnvFiles] = useState<Files>({})
  const [tried, setTried] = useState(false)
  const { convert, conversion, stale } = useConversion(project, yaml, envFiles)
  const { offers, choices, setUnmanaged, setPublished } = useChoices(conversion, context)
  const made = (conversion?.services ?? []).filter((s) => s.image)
  const problems = problemsOf(t, made, choices.published, context.taken)
  const planner = usePlanStep(project, onClose, () => {
    void navigate({ to: "/projects/$project", params: { project } })
  })
  const review = () => {
    setTried(true)
    if (!conversion || problems.size > 0 || made.length === 0) return
    planner.plan({
      title: t("add.composePlan", made.length),
      changes: composeChanges(conversion, choices, { project, ...context }),
      notes: notesOf(t, conversion, offers, choices.managed, envFiles),
    })
  }
  return (
    <PlanDrawer
      step={planner}
      onClose={onClose}
      title={t("add.composeTitle")}
      description={t("add.composeBody", project)}
      footer={
        <Footer
          ready={conversion !== undefined && !stale}
          pending={convert.isPending}
          empty={yaml.trim() === ""}
          onRead={() => convert.mutate()}
          onReview={review}
          onClose={onClose}
        />
      }
    >
      <div className="flex flex-col gap-6">
        <TextFile
          label={t("add.composeFile")}
          hint={t("add.composeHint")}
          upload={t("add.composeUpload")}
          value={yaml}
          rows={10}
          placeholder={"services:\n  web:\n    image: ghcr.io/acme/web:1.4"}
          onChange={setYaml}
          autoFocus
        />
        <EnvFiles
          conversion={conversion}
          files={envFiles}
          onChange={(path, text) => setEnvFiles((files) => ({ ...files, [path]: text }))}
        />
        <ReadError error={convert.error} />
        {stale ? <Note>{t("add.composeChanged")}</Note> : null}
        {conversion ? (
          <Services
            project={project}
            conversion={conversion}
            offers={offers}
            choices={choices}
            suffix={context.domains.suffix}
            publicUrl={context.options.publicUrl}
            problems={tried ? problems : new Map()}
            onManaged={(name, on) => setUnmanaged((names) => toggled(names, name, !on))}
            onHost={(name, host) => setPublished((hosts) => withHost(hosts, name, host))}
          />
        ) : null}
      </div>
    </PlanDrawer>
  )
}

function Footer({
  ready,
  pending,
  empty,
  onRead,
  onReview,
  onClose,
}: {
  ready: boolean
  pending: boolean
  empty: boolean
  onRead: () => void
  onReview: () => void
  onClose: () => void
}) {
  const t = useT()
  return (
    <div className="ml-auto flex gap-2">
      <Button variant="ghost" size="sm" onClick={onClose}>
        {t("edit.cancel")}
      </Button>
      {ready ? (
        <Button size="sm" onClick={onReview}>
          {t("edit.review")}
        </Button>
      ) : (
        <Button size="sm" onClick={onRead} disabled={empty || pending}>
          {pending ? t("add.composeReading") : t("add.composeRead")}
        </Button>
      )}
    </div>
  )
}

const ReadError = ({ error }: { error: Error | null }) =>
  error ? (
    <Alert variant="destructive">
      <AlertDescription>{toProblem(error).detail}</AlertDescription>
    </Alert>
  ) : null

const offersEngine = (offer: Offer): offer is { engine: Managed } =>
  offer !== undefined && "engine" in offer

const keptOf = (offer: Offer): KeptReason | undefined =>
  offer && "kept" in offer ? offer.kept : undefined

const toggled = (names: ReadonlySet<string>, name: string, on: boolean) => {
  const next = new Set(names)
  if (on) next.add(name)
  else next.delete(name)
  return next
}

const withHost = (hosts: ReadonlyMap<string, string>, name: string, host?: string) => {
  const next = new Map(hosts)
  if (host === undefined) next.delete(name)
  else next.set(name, host)
  return next
}

/** Per service that would be made: why its name or domain can't be used. */
function problemsOf(
  t: Translate,
  made: ComposeService[],
  published: ReadonlyMap<string, string>,
  taken: string[],
): Map<string, string> {
  const problems = new Map<string, string>()
  for (const service of made) {
    const name = serviceNameProblem(service.name, taken)
    const host = published.get(service.name)
    const hostWrong = host === undefined ? undefined : hostProblem(host)
    if (name) problems.set(service.name, t(`add.problem.name.${name}`))
    else if (hostWrong) problems.set(service.name, t(`add.problem.host.${hostWrong}`))
  }
  return problems
}

const Note = ({ children }: { children: ReactNode }) => (
  <p className="type-meta text-muted-foreground">{children}</p>
)

/** The env files the Compose file names, and `.env` when a variable it interpolates is unset. */
function EnvFiles({
  conversion,
  files,
  onChange,
}: {
  conversion?: Conversion
  files: Files
  onChange: (path: string, text: string) => void
}) {
  const t = useT()
  const unset = conversion?.unset ?? []
  const needed = [...(conversion?.missingEnvFiles ?? []), ...(unset.length > 0 ? [".env"] : [])]
  const shown = [...new Set([...needed, ...Object.keys(files)])]
  return shown.map((path) => (
    <TextFile
      key={path}
      label={t("add.composeEnvFile", path)}
      upload={t("add.composeUpload")}
      hint={
        path === ".env" && unset.length > 0
          ? t("add.composeUnset", unset.join(t("common.sep")))
          : t("add.composeEnvHint")
      }
      value={files[path] ?? ""}
      rows={4}
      onChange={(text) => onChange(path, text)}
    />
  ))
}

function Services({
  project,
  conversion,
  offers,
  choices,
  suffix,
  publicUrl,
  problems,
  onManaged,
  onHost,
}: {
  project: string
  conversion: Conversion
  offers: Map<string, Offer>
  choices: ComposeChoices
  suffix?: string | null
  publicUrl: boolean
  problems: Map<string, string>
  onManaged: (name: string, on: boolean) => void
  onHost: (name: string, host?: string) => void
}) {
  const t = useT()
  return (
    <div className="flex flex-col gap-2">
      <SectionLabel className="mb-1">
        {t("add.composeServices", conversion.services.length)}
      </SectionLabel>
      <Card variant="list">
        {conversion.services.map((service) => (
          <ServiceRow
            key={service.name}
            service={service}
            offer={offers.get(service.name)}
            managed={choices.managed.has(service.name)}
            onManaged={(on) => onManaged(service.name, on)}
            domain={
              publicUrl ? (
                <PublicDomain
                  host={choices.published.get(service.name)}
                  suggested={suggestedHost(service.name, project, suffix)}
                  placeholder={suffix ? undefined : "web.example.com"}
                  onHost={(host) => onHost(service.name, host)}
                />
              ) : null
            }
            problem={problems.get(service.name)}
          />
        ))}
      </Card>
      {imageServices(conversion, choices).length > 0 ? (
        <Note>{t("add.composeSecurity")}</Note>
      ) : null}
    </div>
  )
}

function ServiceRow({
  service,
  offer,
  managed,
  onManaged,
  domain,
  problem,
}: {
  service: ComposeService
  offer: Offer
  managed: boolean
  onManaged: (on: boolean) => void
  domain: ReactNode
  problem?: string
}) {
  const t = useT()
  const image = service.image ?? ""
  const runs = image !== "" && !managed
  return (
    <CardRow
      align="start"
      label={
        <span className="flex min-w-0 flex-wrap items-baseline gap-x-2">
          <span className="type-mono" translate="no">
            {service.name}
          </span>
          <span className="min-w-0 truncate type-meta text-muted-foreground" translate="no">
            {image}
          </span>
        </span>
      }
      description={
        <span className="flex flex-col gap-2">
          {image ? null : <span>{t("add.composeBuildOnly")}</span>}
          <Counterpart service={service} offer={offer} managed={managed} onManaged={onManaged} />
          {runs ? <ServiceFacts service={service} /> : null}
          {runs && portsOf(service).length > 0 ? domain : null}
          {problem ? (
            <span className="type-meta text-destructive" role="alert">
              {problem}
            </span>
          ) : null}
        </span>
      }
    />
  )
}

/** The operator-managed choice for a database image, or why it runs as it is. */
function Counterpart({
  service,
  offer,
  managed,
  onManaged,
}: {
  service: ComposeService
  offer: Offer
  managed: boolean
  onManaged: (on: boolean) => void
}) {
  const t = useT()
  const kept = keptOf(offer)
  const image = service.image ?? ""
  if (kept && image) return <span>{keptText(t, kept, image, service)}</span>
  if (!offersEngine(offer)) return null
  return (
    <>
      <Toggle checked={managed} onChange={onManaged}>
        {t("add.composeManaged", managedName(t, offer.engine))}
      </Toggle>
      {managed ? <span>{t("add.composeManagedHint", image)}</span> : null}
    </>
  )
}

function Toggle({
  checked,
  onChange,
  children,
}: {
  checked: boolean
  onChange: (on: boolean) => void
  children: ReactNode
}) {
  const id = useId()
  return (
    <span className="flex items-center gap-2">
      <Switch id={id} size="sm" checked={checked} onCheckedChange={onChange} />
      <Label htmlFor={id}>{children}</Label>
    </span>
  )
}

function PublicDomain({
  host,
  suggested,
  placeholder,
  onHost,
}: {
  host?: string
  suggested: string
  placeholder?: string
  onHost: (host?: string) => void
}) {
  const t = useT()
  return (
    <span className="flex flex-col gap-2">
      <Toggle checked={host !== undefined} onChange={(on) => onHost(on ? suggested : undefined)}>
        {t("add.publicUrl")}
      </Toggle>
      {host === undefined ? null : (
        <Input
          size="sm"
          mono
          aria-label={t("add.publicUrl")}
          value={host}
          placeholder={placeholder}
          onChange={(event) => onHost(event.target.value.trim())}
        />
      )}
    </span>
  )
}

/** Ports, volumes, what it waits for, and what is left out. */
function ServiceFacts({ service }: { service: ComposeService }) {
  const t = useT()
  const ports = portsOf(service)
  const sep = t("common.sep")
  const lines = [
    ports.length > 0 ? t("add.composePorts", ports.join(sep)) : "",
    service.volumes.length > 0
      ? t("add.composeVolumes", service.volumes.map((v) => v.mountPath).join(sep))
      : "",
    service.dependsOn.length > 0 ? t("add.composeWaits", service.dependsOn.join(sep)) : "",
    ...service.findings.map((f) => findingText(t, f, service)),
  ].filter(Boolean)
  return lines.map((line) => (
    <span key={line} className="type-meta text-muted-foreground">
      {line}
    </span>
  ))
}

const managedName = (t: Translate, engine: Managed) =>
  engine === "objectStorage" ? t("s3.engine") : engineName(engine)

const KEPT: Record<KeptReason, (t: Translate, image: string, engine: string) => string> = {
  notInstalled: (t, image, engine) => t("add.composeKept.notInstalled", image, engine),
  unknown: (t, image, engine) => t("add.composeKept.unknown", image, engine),
  role: (t, image) => t("add.composeKept.role", image),
}

function keptText(t: Translate, kept: KeptReason, image: string, service: ComposeService) {
  const counterpart = service.counterpart
  if (kept === "role" && counterpart === "objectStorage")
    return t("add.composeKept.roleObjectStorage", image)
  return KEPT[kept](t, image, counterpart ? managedName(t, counterpart) : "")
}

type Of<C extends Finding["code"]> = Extract<Finding, { code: C }>
const FINDING: {
  [C in Finding["code"]]: (t: Translate, finding: Of<C>, service: ComposeService) => string
} = {
  privileged: (t) => t("add.composeFinding.privileged"),
  hostNetwork: (t) => t("add.composeFinding.hostNetwork"),
  hostPid: (t) => t("add.composeFinding.hostPid"),
  devices: (t) => t("add.composeFinding.devices"),
  healthcheck: (t) => t("add.composeFinding.healthcheck"),
  capAdd: (t, f) => t("add.composeFinding.capAdd", f.capabilities.join(t("common.sep"))),
  dockerSocket: (t, f) => t("add.composeFinding.dockerSocket", f.source),
  hostPath: (t, f) => t("add.composeFinding.hostPath", f.source, f.target),
  build: (t, _, s) =>
    s.image ? t("add.composeFinding.build", s.image) : t("add.composeBuildOnly"),
  udpPort: (t, f) => t("add.composeFinding.udpPort", String(f.port)),
  sharedVolume: (t, f) => t("add.composeFinding.sharedVolume", f.volume),
  renamed: (t, _, s) => t("add.composeFinding.renamed", s.composeName, s.name),
}

const findingText = (t: Translate, finding: Finding, service: ComposeService) =>
  (FINDING[finding.code] as (t: Translate, f: Finding, s: ComposeService) => string)(
    t,
    finding,
    service,
  )

/** One service's lines for the plan: why it stays an image, or what is left out of it. */
function serviceNotes(
  t: Translate,
  service: ComposeService,
  offer: Offer,
  managed: boolean,
): string[] {
  const kept = keptOf(offer)
  if (managed) return [t("add.composeManagedHint", service.image ?? "")]
  return [
    ...(kept && service.image ? [keptText(t, kept, service.image, service)] : []),
    ...service.findings.map((f) => findingText(t, f, service)),
    ...service.variables
      .filter((v) => v.refers?.part === "mention")
      .map((v) => t("add.composeMention", v.name, v.refers?.service ?? "")),
  ]
}

/** What the plan cannot say itself: what is left out and why. */
function notesOf(
  t: Translate,
  conversion: Conversion,
  offers: Map<string, Offer>,
  managed: ReadonlySet<string>,
  envFiles: Files,
): string[] {
  const sep = t("common.sep")
  const given = (path: string) => (envFiles[path] ?? "").trim() !== ""
  const missing = conversion.missingEnvFiles.filter((path) => !given(path))
  return [
    ...conversion.services.flatMap((s) =>
      serviceNotes(t, s, offers.get(s.name), managed.has(s.name)).map(
        (line) => `${s.name}${t("common.colon")}${line}`,
      ),
    ),
    ...(missing.length > 0 ? [t("add.composeNoEnvFile", missing.join(sep))] : []),
    ...(conversion.unset.length > 0 && !given(".env")
      ? [t("add.composeUnset", conversion.unset.join(sep))]
      : []),
  ]
}
