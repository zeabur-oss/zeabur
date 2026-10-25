// The fields "Deploy from Git" and "Upload a folder" share: a service whose image a Zeabur Builder
// Build makes. The hook keeps the form and what is derived from it; the pieces draw a field each,
// so each drawer lays them out around its own source.
import { useQuery } from "@tanstack/react-query"
import { useId, useState, type ReactNode } from "react"
import CaretRightIcon from "~icons/ph/caret-right"

import { clusterQuery, creatableQuery } from "@/api/queries"
import { ImageSizeField } from "@/components/add/image-size"
import { SecurityProfileField } from "@/components/security-profile"
import { useTakenNames } from "@/components/add/taken"
import { FormField } from "@/components/form-field"
import { useT } from "@/i18n"
import {
  buildMethods,
  buildProfile,
  type BuildMethod,
  type BuiltService,
  type GitService,
} from "@/model/git-service"
import { imageOptions, parseEnv, rightsOf, suggestedHost } from "@/model/image-service"
import type { Profile } from "@/model/pod-security"
import type { FieldProblem } from "@/model/image-service"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@zeabur/design-system/components/collapsible"
import { Input } from "@zeabur/design-system/components/input"
import { Textarea } from "@zeabur/design-system/components/textarea"
import { Label } from "@zeabur/design-system/components/label"
import { Select } from "@zeabur/design-system/components/select"
import { Switch } from "@zeabur/design-system/components/switch"

export type BuiltForm = BuiltService & Pick<GitService, "directory" | "dockerfile">

export const Note = ({ children }: { children: ReactNode }) => (
  <p className="type-meta text-muted-foreground">{children}</p>
)

export type Shown = (problem: unknown, text: () => string) => string[]

interface Choices {
  named: boolean
  hosted: boolean
  securitySet: boolean
}

/** The form as the objects take it, with what the person has not typed filled in. */
function resolveBuilt<F extends BuiltForm>(
  form: F,
  choices: Choices,
  context: {
    project: string
    suggested: string
    options: ReturnType<typeof imageOptions>
    suffix: string | undefined
    env: ReturnType<typeof parseEnv>
    buildEnv: ReturnType<typeof parseEnv>
  },
) {
  const name = choices.named ? form.name : context.suggested
  const port = context.options.port ? form.port.trim() : ""
  const publicUrl = context.options.publicUrl && port !== "" && form.publicUrl
  const host = choices.hosted ? form.host : suggestedHost(name, context.project, context.suffix)
  const security = choices.securitySet ? form.security : buildProfile(form.method)
  const resolved: F = {
    ...form,
    name,
    port,
    publicUrl,
    host,
    variables: context.env.variables,
    dockerfile: form.method === "dockerfile" ? form.dockerfile : "",
    buildVariables: context.buildEnv.variables.flatMap((v) =>
      "value" in v ? [{ name: v.name, value: v.value }] : [],
    ),
    security,
  }
  return { name, port, publicUrl, host, security, resolved }
}

/** `suggested` gives the name until the person types one, from the form and the names taken. */
export function useBuiltForm<F extends BuiltForm>(
  project: string,
  initial: () => F,
  suggested: (form: F, taken: string[]) => string,
) {
  const cluster = useQuery(clusterQuery).data
  const domains = cluster?.domains ?? {}
  const options = imageOptions(rightsOf(useQuery(creatableQuery(project)).data))
  const taken = useTakenNames(project)
  const [form, setForm] = useState<F>(initial)
  const [env, setEnv] = useState("")
  const [buildEnv, setBuildEnv] = useState("")
  const [choices, setChoices] = useState<Choices>({
    named: false,
    hosted: false,
    securitySet: false,
  })
  const [tried, setTried] = useState(false)
  const set = (patch: Partial<F>) => setForm((f) => ({ ...f, ...patch }))
  const typed = (choice: keyof Choices, patch: Partial<F>) => {
    setChoices((c) => ({ ...c, [choice]: true }))
    set(patch)
  }
  const parsedEnv = parseEnv(env)
  const parsedBuildEnv = parseEnv(buildEnv)
  const derived = resolveBuilt(form, choices, {
    project,
    suggested: suggested(form, taken),
    options,
    suffix: domains.suffixes?.[0],
    env: parsedEnv,
    buildEnv: parsedBuildEnv,
  })
  return {
    ...derived,
    project,
    form,
    set,
    registry: cluster?.builds.registry ?? "",
    domains,
    options,
    taken,
    env,
    setEnv,
    buildEnv,
    setBuildEnv,
    badEnv: parsedEnv.bad,
    badBuildEnv: parsedBuildEnv.bad,
    typeName: (name: string) => typed("named", { name } as Partial<F>),
    typeHost: (host: string) => typed("hosted", { host } as Partial<F>),
    chooseSecurity: (security: F["security"]) => typed("securitySet", { security } as Partial<F>),
    tried,
    try: () => setTried(true),
    shown: ((problem, text) => (tried && problem ? [text()] : [])) as Shown,
  }
}

export type BuiltFormState = ReturnType<typeof useBuiltForm<BuiltForm>>

interface Problems {
  name: FieldProblem | undefined
  port: unknown
  host: "empty" | "invalid" | undefined
  directory: unknown
  dockerfile: unknown
  env: boolean
  buildEnv: boolean
}

export function ServiceNameField({
  built,
  problems,
}: {
  built: BuiltFormState
  problems: Problems
}) {
  const t = useT()
  return (
    <FormField
      label={t("db.field.name")}
      errors={built.shown(problems.name, () => t(`add.problem.name.${problems.name ?? "empty"}`))}
    >
      <Input
        size="sm"
        mono
        value={built.name}
        autoComplete="off"
        spellCheck={false}
        onChange={(event) => built.typeName(event.target.value.trim())}
      />
    </FormField>
  )
}

export function MethodField({
  built,
  source,
}: {
  built: BuiltFormState
  source: "git" | "upload"
}) {
  const t = useT()
  const method = built.form.method
  return (
    <FormField
      label={t("add.field.method")}
      hint={t(source === "upload" ? `upload.methodHint.${method}` : `add.methodHint.${method}`)}
    >
      <Select
        size="sm"
        value={built.form.method}
        onValueChange={(method) => built.set({ method: method as BuildMethod })}
        items={buildMethods.map((m) => ({ value: m, label: t(`add.method.${m}`) }))}
      />
    </FormField>
  )
}

/** The port the app listens on. */
export function PortField({
  value,
  onChange,
  hint,
  error,
}: {
  value: string
  onChange: (port: string) => void
  hint: ReactNode
  error: string[]
}) {
  const t = useT()
  return (
    <FormField label={t("add.field.port")} hint={hint} errors={error}>
      <Input
        size="sm"
        mono
        inputMode="numeric"
        value={value}
        placeholder="8080"
        onChange={(event) => onChange(event.target.value)}
      />
    </FormField>
  )
}

/** A switch with its label. */
export function Toggle({
  checked,
  onChange,
  label,
}: {
  checked: boolean
  onChange: (checked: boolean) => void
  label: string
}) {
  const id = useId()
  return (
    <div className="flex items-center gap-2">
      <Switch id={id} size="sm" checked={checked} onCheckedChange={onChange} />
      <Label htmlFor={id}>{label}</Label>
    </div>
  )
}

/** "Public URL": the switch and, when on, the host; nothing for a service with no port. */
export function PublicUrlFields({
  port,
  canPublish,
  on,
  onToggle,
  host,
  onHost,
  domains,
  error,
}: {
  port: string
  canPublish: boolean
  on: boolean
  onToggle: (on: boolean) => void
  host: string
  onHost: (host: string) => void
  domains: { suffixes?: string[]; clusterIssuer?: string | null }
  error: string[]
}) {
  const t = useT()
  if (port === "") return null
  if (!canPublish) return <Note>{t("add.noPublicUrl")}</Note>
  return (
    <div className="flex flex-col gap-3">
      <Toggle checked={on} onChange={onToggle} label={t("add.publicUrl")} />
      {on && (
        <FormField
          label={t("add.publicUrl")}
          hideLabel
          hint={
            <>
              {domains.suffixes?.length ? null : <span className="block">{t("add.hostHint")}</span>}
              <span className="block">
                {domains.clusterIssuer ? t("add.https", domains.clusterIssuer) : t("add.httpOnly")}
              </span>
            </>
          }
          errors={error}
        >
          <Input
            size="sm"
            mono
            value={host}
            placeholder={domains.suffixes?.length ? undefined : "web.example.com"}
            autoComplete="off"
            spellCheck={false}
            onChange={(event) => onHost(event.target.value.trim())}
          />
        </FormField>
      )}
    </div>
  )
}

/** "More settings": the directory, Dockerfile, variables, size and security. */
export function MoreSection({
  built,
  problems,
  source,
}: {
  built: BuiltFormState
  problems: Problems
  source: "git" | "upload"
}) {
  const t = useT()
  const [shown, setShown] = useState(false)
  const open =
    built.tried &&
    (!!problems.directory || !!problems.dockerfile || problems.env || problems.buildEnv)
  return (
    <Collapsible open={shown || open} onOpenChange={setShown}>
      <CollapsibleTrigger variant="disclosure">
        <CaretRightIcon aria-hidden="true" />
        {t("add.more")}
      </CollapsibleTrigger>
      {shown || open ? null : (
        <p className="mt-1 type-meta text-muted-foreground">
          {t(source === "upload" ? "upload.moreSummary" : "add.gitMoreSummary")}
        </p>
      )}
      <CollapsibleContent>
        <MoreFields
          project={built.project}
          source={source}
          form={built.form}
          set={built.set}
          security={built.security}
          onSecurity={built.chooseSecurity}
          env={built.env}
          onEnv={built.setEnv}
          buildEnv={built.buildEnv}
          onBuildEnv={built.setBuildEnv}
          problems={problems}
          bad={{ env: built.badEnv, buildEnv: built.badBuildEnv }}
          shown={built.shown}
        />
      </CollapsibleContent>
    </Collapsible>
  )
}

type BuildFields = Pick<GitService, "directory" | "dockerfile" | "method" | "size">

/** The fields behind "More settings": where and how to build, variables, size and security. */
function MoreFields({
  project,
  source = "git",
  form,
  set,
  security,
  onSecurity,
  env,
  onEnv,
  buildEnv,
  onBuildEnv,
  problems,
  bad,
  shown,
}: {
  project: string
  /** What the directory is inside: the repository or the uploaded folder. */
  source?: "git" | "upload"
  form: BuildFields
  set: (patch: Partial<BuildFields>) => void
  security: Profile
  onSecurity: (profile: Profile) => void
  env: string
  onEnv: (env: string) => void
  buildEnv: string
  onBuildEnv: (env: string) => void
  problems: Pick<Problems, "directory" | "dockerfile" | "env" | "buildEnv">
  bad: { env: number[]; buildEnv: number[] }
  shown: (problem: unknown, text: () => string) => string[]
}) {
  const t = useT()
  return (
    <div className="flex flex-col gap-4 pt-4">
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
        <FormField
          label={t("add.field.directory")}
          hint={t(source === "upload" ? "upload.directoryHint" : "add.directoryHint")}
          errors={shown(problems.directory, () =>
            t(source === "upload" ? "upload.problem.directory" : "add.problem.directory"),
          )}
        >
          <Input
            size="sm"
            mono
            value={form.directory}
            placeholder="apps/web"
            autoComplete="off"
            spellCheck={false}
            onChange={(event) => set({ directory: event.target.value })}
          />
        </FormField>
        {form.method === "dockerfile" ? (
          <FormField
            label={t("add.field.dockerfile")}
            hint={t("add.dockerfileHint")}
            errors={shown(problems.dockerfile, () =>
              t(source === "upload" ? "upload.problem.dockerfile" : "add.problem.dockerfile"),
            )}
          >
            <Input
              size="sm"
              mono
              value={form.dockerfile}
              placeholder="Dockerfile"
              autoComplete="off"
              spellCheck={false}
              onChange={(event) => set({ dockerfile: event.target.value })}
            />
          </FormField>
        ) : null}
      </div>
      <FormField
        label={t("add.variables")}
        hint={t("add.variablesHint")}
        errors={shown(problems.env, () => t("add.variablesBad", bad.env.join(", ")))}
      >
        <Textarea
          mono
          rows={4}
          value={env}
          placeholder={"DATABASE_URL=postgres://…\nLOG_LEVEL=info"}
          spellCheck={false}
          onChange={(event) => onEnv(event.target.value)}
        />
      </FormField>
      <FormField
        label={t("add.buildVariables")}
        hint={
          <>
            <span className="block">{t("add.buildVariablesHint")}</span>
            {form.method === "dockerfile" ? (
              <span className="block">{t("add.buildVariablesDockerfile")}</span>
            ) : null}
          </>
        }
        errors={shown(problems.buildEnv, () => t("add.variablesBad", bad.buildEnv.join(", ")))}
      >
        <Textarea
          mono
          rows={3}
          value={buildEnv}
          placeholder="NPM_TOKEN=…"
          spellCheck={false}
          onChange={(event) => onBuildEnv(event.target.value)}
        />
      </FormField>
      <ImageSizeField size={form.size} onSize={(size) => set({ size })} />
      <SecurityProfileField project={project} built value={security} onChange={onSecurity} />
    </div>
  )
}

/** The port field of a service built by Zeabur Builder; nothing where the person may not set one. */
export function BuiltPort({ built, problems }: { built: BuiltFormState; problems: Problems }) {
  const t = useT()
  if (!built.options.port) return null
  return (
    <PortField
      value={built.form.port}
      onChange={(port) => built.set({ port })}
      hint={t("add.gitPortHint")}
      error={built.shown(problems.port, () => t("add.problem.port.invalid"))}
    />
  )
}

/** The public URL of a service built by Zeabur Builder. */
export function BuiltPublicUrl({ built, problems }: { built: BuiltFormState; problems: Problems }) {
  const t = useT()
  return (
    <PublicUrlFields
      port={built.port}
      canPublish={built.options.publicUrl}
      on={built.form.publicUrl}
      onToggle={(publicUrl) => built.set({ publicUrl })}
      host={built.host}
      onHost={built.typeHost}
      domains={built.domains}
      error={built.shown(problems.host, () => t(`add.problem.host.${problems.host ?? "empty"}`))}
    />
  )
}
