import { useMutation, useQuery } from "@tanstack/react-query"
import { useNavigate } from "@tanstack/react-router"
import { useId, useState, type ReactNode } from "react"
import ArrowSquareOutIcon from "~icons/ph/arrow-square-out"
import CaretRightIcon from "~icons/ph/caret-right"

import { api, toProblem, unwrap } from "@/api/client"
import { clusterQuery, creatableQuery, projectQuery } from "@/api/queries"
import { FormField } from "@/components/form-field"
import { PlanDrawer } from "@/components/plan/plan-drawer"
import { usePlanStep } from "@/components/plan/planner"
import { useT } from "@/i18n"
import { serviceLink } from "@/lib/place"
import {
  branchProblem,
  buildMethods,
  createGitService,
  deployKeyHost,
  deployKeyPage,
  directoryProblem,
  newGitService,
  parseRepository,
  tokenUser,
  urlProblem,
  type Access,
  type BuildMethod,
  type GitService,
} from "@/model/git-service"
import {
  hostProblem,
  imageOptions,
  imageResources,
  imageSizes,
  nameFromImage,
  parseEnv,
  portProblem,
  rightsOf,
  serviceNameProblem,
  suggestedHost,
  type ImageSize,
} from "@/model/image-service"
import { Alert, AlertDescription } from "@zeabur/design-system/components/alert"
import { Button } from "@zeabur/design-system/components/button"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@zeabur/design-system/components/collapsible"
import { CopyInput } from "@zeabur/design-system/components/copy-input"
import { Input } from "@zeabur/design-system/components/input"
import { Label } from "@zeabur/design-system/components/label"
import { RadioGroup, RadioOption } from "@zeabur/design-system/components/radio"
import { Select } from "@zeabur/design-system/components/select"
import { Switch } from "@zeabur/design-system/components/switch"
import { Textarea } from "@zeabur/design-system/components/textarea"

interface DeployKey {
  host: string
  publicKey: string
  key: string
}

/** "Deploy from Git": Zeabur Builder's Build, the service that runs what it builds, and the
 * repository's credentials, in one plan. */
export function GitDrawer({ project, onClose }: { project: string; onClose: () => void }) {
  const t = useT()
  const navigate = useNavigate()
  const cluster = useQuery(clusterQuery).data
  const domains = cluster?.domains ?? {}
  const registry = cluster?.builds.registry ?? ""
  const options = imageOptions(rightsOf(useQuery(creatableQuery(project)).data))
  const view = useQuery(projectQuery(project)).data
  const taken = view
    ? [
        ...view.services.map((s) => s.name),
        ...view.endpoints.map((e) => e.name),
        ...view.routes.map((r) => r.name),
      ]
    : []
  const [form, setForm] = useState<GitService>(newGitService)
  const [env, setEnv] = useState("")
  const [named, setNamed] = useState(false)
  const [hosted, setHosted] = useState(false)
  const [userSet, setUserSet] = useState(false)
  const [tried, setTried] = useState(false)
  const [moreShown, setMoreShown] = useState(false)
  const [deployKey, setDeployKey] = useState<DeployKey>()
  const ids = { publicUrl: useId() }
  const set = (patch: Partial<GitService>) => setForm((f) => ({ ...f, ...patch }))

  const repo = parseRepository(form.url)
  const keyHost = deployKeyHost(repo)
  const secrets = options.secrets
  const access: Access =
    !secrets || (form.access === "deployKey" && !keyHost) ? "public" : form.access
  const name = named
    ? form.name
    : repo
      ? nameFromImage(repo.path.split("/").at(-1) ?? "", taken)
      : ""
  const port = options.port ? form.port.trim() : ""
  const publicUrl = options.publicUrl && port !== "" && form.publicUrl
  const host = hosted ? form.host : suggestedHost(name, project, domains.suffix)
  const variables = parseEnv(env)
  const username = userSet ? form.username : tokenUser(repo?.host)
  const key = deployKey && deployKey.host === keyHost ? deployKey : undefined

  const makeKey = useMutation({
    mutationFn: async (gitHost: string) =>
      unwrap(
        await api.POST("/api/v1/projects/{project}/deploy-keys", {
          params: { path: { project } },
          body: { host: gitHost },
        }),
      ),
    onSuccess: (made, gitHost) => setDeployKey({ host: gitHost, ...made }),
  })

  const problems = {
    url: urlProblem(form.url),
    branch: branchProblem(form.branch),
    name: serviceNameProblem(name, taken),
    port: portProblem(port),
    host: publicUrl ? hostProblem(host) : undefined,
    directory: directoryProblem(form.directory),
    env: variables.bad.length > 0,
    key: access === "deployKey" && !key,
    token: access === "token" && (form.token.trim() === "" || username.trim() === ""),
  }
  const blocked = Object.values(problems).some(Boolean)
  const shown = (problem: unknown, text: () => string) => (tried && problem ? [text()] : [])

  const planner = usePlanStep(project, onClose, () => {
    void navigate(serviceLink(project, { kind: "Deployment", name }))
  })
  const review = () => {
    setTried(true)
    if (blocked) return
    planner.plan({
      title: t("add.planGit", name),
      changes: createGitService(
        {
          ...form,
          access,
          name,
          port,
          publicUrl,
          host,
          username,
          variables: variables.variables,
          sealedKey: key?.key,
        },
        project,
        registry,
        domains,
      ),
    })
  }

  const resources = imageResources[form.size]
  const moreOpen = tried && (!!problems.directory || problems.env)
  const keyPage = repo ? deployKeyPage(repo) : undefined

  return (
    <PlanDrawer
      step={planner}
      onClose={onClose}
      title={t("add.gitTitle")}
      description={t("add.gitBody", project)}
      footer={
        <div className="ml-auto flex gap-2">
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
        className="flex flex-col gap-4"
        onSubmit={(event) => {
          event.preventDefault()
          review()
        }}
      >
        <FormField
          label={t("add.field.repository")}
          errors={shown(problems.url, () => t(`add.problem.repository.${problems.url ?? "empty"}`))}
        >
          <Input
            size="sm"
            mono
            value={form.url}
            placeholder="https://github.com/acme/web"
            autoComplete="off"
            spellCheck={false}
            autoFocus
            onChange={(event) => set({ url: event.target.value })}
          />
        </FormField>
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
          <FormField
            label={t("add.field.branch")}
            hint={t("add.branchHint")}
            errors={shown(problems.branch, () => t("add.problem.branch"))}
          >
            <Input
              size="sm"
              mono
              value={form.branch}
              placeholder={t("add.branchDefault")}
              autoComplete="off"
              spellCheck={false}
              onChange={(event) => set({ branch: event.target.value })}
            />
          </FormField>
          <FormField label={t("add.field.method")} hint={t(`add.methodHint.${form.method}`)}>
            <Select
              size="sm"
              value={form.method}
              onValueChange={(method) => set({ method: method as BuildMethod })}
              items={buildMethods.map((m) => ({ value: m, label: t(`add.method.${m}`) }))}
            />
          </FormField>
        </div>
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
          <FormField
            label={t("db.field.name")}
            errors={shown(problems.name, () => t(`add.problem.name.${problems.name ?? "empty"}`))}
          >
            <Input
              size="sm"
              mono
              value={name}
              autoComplete="off"
              spellCheck={false}
              onChange={(event) => {
                setNamed(true)
                set({ name: event.target.value.trim() })
              }}
            />
          </FormField>
          {options.port ? (
            <FormField
              label={t("add.field.port")}
              hint={t("add.gitPortHint")}
              errors={shown(problems.port, () => t("add.problem.port.invalid"))}
            >
              <Input
                size="sm"
                mono
                inputMode="numeric"
                value={form.port}
                placeholder="8080"
                onChange={(event) => set({ port: event.target.value })}
              />
            </FormField>
          ) : null}
        </div>
        {port === "" ? null : options.publicUrl ? (
          <div className="flex flex-col gap-3">
            <div className="flex items-center gap-2">
              <Switch
                id={ids.publicUrl}
                size="sm"
                checked={form.publicUrl}
                onCheckedChange={(on) => set({ publicUrl: on })}
              />
              <Label htmlFor={ids.publicUrl}>{t("add.publicUrl")}</Label>
            </div>
            {publicUrl ? (
              <FormField
                label={t("add.publicUrl")}
                hideLabel
                hint={
                  <>
                    {domains.suffix ? null : <span className="block">{t("add.hostHint")}</span>}
                    <span className="block">
                      {domains.clusterIssuer
                        ? t("add.https", domains.clusterIssuer)
                        : t("add.httpOnly")}
                    </span>
                  </>
                }
                errors={shown(problems.host, () =>
                  t(`add.problem.host.${problems.host ?? "empty"}`),
                )}
              >
                <Input
                  size="sm"
                  mono
                  value={host}
                  placeholder={domains.suffix ? undefined : "web.example.com"}
                  autoComplete="off"
                  spellCheck={false}
                  onChange={(event) => {
                    setHosted(true)
                    set({ host: event.target.value.trim() })
                  }}
                />
              </FormField>
            ) : null}
          </div>
        ) : (
          <Note>{t("add.noPublicUrl")}</Note>
        )}
        <fieldset className="flex min-w-0 flex-col gap-2">
          <legend className="mb-1.5 text-sm font-medium">{t("add.access")}</legend>
          {secrets ? (
            <RadioGroup value={access} onValueChange={(value) => set({ access: value as Access })}>
              <RadioOption value="public" label={t("add.access.public")} />
              <RadioOption
                value="deployKey"
                disabled={!keyHost}
                label={t("add.access.deployKey")}
                description={
                  keyHost ? t("add.access.deployKeyHint") : t("add.access.deployKeyHosts")
                }
              />
              <RadioOption
                value="token"
                label={t("add.access.token")}
                description={t("add.access.tokenHint")}
              />
            </RadioGroup>
          ) : (
            <Note>{t("add.access.noSecrets")}</Note>
          )}
          {access === "deployKey" && keyHost ? (
            <div className="flex flex-col gap-2 pt-2">
              {key ? (
                <>
                  <FormField label={t("add.deployKeyPublic")}>
                    <CopyInput value={key.publicKey} copyLabel={t("common.copy")} />
                  </FormField>
                  <p className="flex flex-wrap items-baseline gap-x-2 type-meta text-muted-foreground">
                    {t("add.deployKeyAdd")}
                    {keyPage ? (
                      <a
                        href={keyPage}
                        target="_blank"
                        rel="noreferrer"
                        className="inline-flex items-center gap-1 text-foreground hover:underline"
                      >
                        {t("add.deployKeyOpen", keyHost === "github.com" ? "GitHub" : "GitLab")}
                        <ArrowSquareOutIcon aria-hidden="true" className="size-3.5" />
                      </a>
                    ) : null}
                  </p>
                </>
              ) : (
                <div className="flex flex-col gap-1.5">
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    className="self-start"
                    disabled={makeKey.isPending}
                    onClick={() => makeKey.mutate(keyHost)}
                  >
                    {t("add.deployKeyMake")}
                  </Button>
                  {tried && problems.key ? (
                    <p className="text-xs leading-4 text-destructive">
                      {t("add.problem.deployKey")}
                    </p>
                  ) : null}
                </div>
              )}
              {makeKey.isError ? (
                <Alert variant="destructive">
                  <AlertDescription>{toProblem(makeKey.error).detail}</AlertDescription>
                </Alert>
              ) : null}
            </div>
          ) : null}
          {access === "token" ? (
            <div className="grid grid-cols-1 gap-4 pt-2 sm:grid-cols-2">
              <FormField label={t("add.registryUser")}>
                <Input
                  size="sm"
                  value={username}
                  autoComplete="off"
                  onChange={(event) => {
                    setUserSet(true)
                    set({ username: event.target.value })
                  }}
                />
              </FormField>
              <FormField
                label={t("add.token")}
                errors={shown(problems.token, () => t("add.problem.token"))}
              >
                <Input
                  size="sm"
                  type="password"
                  value={form.token}
                  autoComplete="new-password"
                  onChange={(event) => set({ token: event.target.value })}
                />
              </FormField>
            </div>
          ) : null}
        </fieldset>
        <Collapsible open={moreShown || moreOpen} onOpenChange={setMoreShown}>
          <CollapsibleTrigger variant="disclosure">
            <CaretRightIcon aria-hidden="true" />
            {t("add.more")}
          </CollapsibleTrigger>
          {moreShown || moreOpen ? null : (
            <p className="mt-1 type-meta text-muted-foreground">{t("add.gitMoreSummary")}</p>
          )}
          <CollapsibleContent>
            <div className="flex flex-col gap-4 pt-4">
              <FormField
                label={t("add.field.directory")}
                hint={t("add.directoryHint")}
                errors={shown(problems.directory, () => t("add.problem.directory"))}
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
              <FormField
                label={t("add.variables")}
                hint={t("add.variablesHint")}
                errors={shown(problems.env, () => t("add.variablesBad", variables.bad.join(", ")))}
              >
                <Textarea
                  mono
                  rows={4}
                  value={env}
                  placeholder={"DATABASE_URL=postgres://…\nLOG_LEVEL=info"}
                  spellCheck={false}
                  onChange={(event) => setEnv(event.target.value)}
                />
              </FormField>
              <FormField
                label={t("db.field.size")}
                hint={t(
                  "db.sizeDetail",
                  resources.requests.cpu,
                  resources.requests.memory,
                  resources.limits.memory,
                )}
              >
                <Select
                  size="sm"
                  value={form.size}
                  onValueChange={(size) => set({ size: size as ImageSize })}
                  items={imageSizes.map((p) => ({ value: p, label: t(`db.size.${p}`) }))}
                />
              </FormField>
            </div>
          </CollapsibleContent>
        </Collapsible>
        <Note>{t("add.gitAfter")}</Note>
      </form>
    </PlanDrawer>
  )
}

const Note = ({ children }: { children: ReactNode }) => (
  <p className="type-meta text-muted-foreground">{children}</p>
)
