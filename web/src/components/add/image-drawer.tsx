import { useQuery } from "@tanstack/react-query"
import { useNavigate } from "@tanstack/react-router"
import { useState } from "react"
import CaretRightIcon from "~icons/ph/caret-right"

import { clusterQuery, creatableQuery } from "@/api/queries"
import { useTakenNames } from "@/components/add/taken"
import { AmountInput } from "@/components/amount-input"
import { FormField, quantity } from "@/components/form-field"
import { PlanDrawer } from "@/components/plan/plan-drawer"
import { PortNoteHint, useImageInspection } from "@/components/add/image-port"
import { Note, PortField, PublicUrlFields, Toggle } from "@/components/add/built-service"
import { ImageSizeField } from "@/components/add/image-size"
import { usePlanStep } from "@/components/plan/planner"
import { SecurityProfileField } from "@/components/security-profile"
import { growthWords, useStorage } from "@/components/storage-class"
import { useT } from "@/i18n"
import { handOff } from "@/lib/adding"
import { serviceLink } from "@/lib/place"
import { toYaml } from "@/lib/yaml-out"
import { portNote, shownPort } from "@/model/image-ports"
import {
  createImageService,
  hasTag,
  hostProblem,
  imageObjects,
  imageOptions,
  imageProblem,
  nameFromImage,
  newImageService,
  parseEnv,
  portProblem,
  rightsOf,
  serviceNameProblem,
  suggestedHost,
  type ImageService,
} from "@/model/image-service"
import { imageProfile, type Profile } from "@/model/pod-security"
import { Button } from "@zeabur/design-system/components/button"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@zeabur/design-system/components/collapsible"
import { Input } from "@zeabur/design-system/components/input"
import { Select } from "@zeabur/design-system/components/select"
import { Textarea } from "@zeabur/design-system/components/textarea"

const MOUNT_PATH = /^\/\S*$/
const MAX_REPLICAS = 20

interface Extras {
  env: string
  volume: boolean
  mountPath: string
  volumeSize: string
  registry: boolean
  server: string
  username: string
  password: string
}

const newExtras = (): Extras => ({
  env: "",
  volume: false,
  mountPath: "/data",
  volumeSize: "1Gi",
  registry: false,
  server: "",
  username: "",
  password: "",
})

/** "Docker image": a Deployment, and a Service, Ingress, volume and Secrets as the form asks, in
 * one plan. */
export function ImageDrawer({ project, onClose }: { project: string; onClose: () => void }) {
  const t = useT()
  const storage = useStorage(project)
  const navigate = useNavigate()
  const domains = useQuery(clusterQuery).data?.domains ?? {}
  const options = imageOptions(rightsOf(useQuery(creatableQuery(project)).data))
  const taken = useTakenNames(project)
  const [form, setForm] = useState<ImageService>(newImageService)
  const [extras, setExtras] = useState<Extras>(newExtras)
  const [named, setNamed] = useState(false)
  const [hosted, setHosted] = useState(false)
  const [tried, setTried] = useState(false)
  const [moreShown, setMoreShown] = useState(false)
  const set = (patch: Partial<ImageService>) => setForm((f) => ({ ...f, ...patch }))
  const more = (patch: Partial<Extras>) => setExtras((e) => ({ ...e, ...patch }))

  const name = named ? form.name : form.image.trim() ? nameFromImage(form.image, taken) : ""
  const [typedPort, setTypedPort] = useState<string>()
  const login = options.secrets && extras.registry ? registryLogin(extras) : undefined
  const inspection = useImageInspection(project, {
    image: form.image.trim(),
    registry: login,
  }).data
  const { security, picked, choose } = useImageSecurity(inspection)
  const portValue = shownPort(typedPort, inspection)
  const note = portNote(portValue, inspection)
  const port = options.port ? portValue.trim() : ""
  const publicUrl = options.publicUrl && port !== "" && form.publicUrl
  const host = hosted ? form.host : suggestedHost(name, project, domains.suffixes?.[0])
  const env = parseEnv(extras.env)
  const secrets = options.secrets
  const volume = options.volume && extras.volume
  const registry = secrets && extras.registry

  const service = (): ImageService => ({
    ...form,
    security,
    name,
    port,
    publicUrl,
    host,
    variables: env.variables,
    secretVariables: secrets && form.secretVariables,
    volumes: volume
      ? [{ name: "data", mountPath: extras.mountPath.trim(), size: extras.volumeSize }]
      : undefined,
    replicas: volume ? 1 : form.replicas,
    registry: registry
      ? { server: extras.server.trim(), username: extras.username, password: extras.password }
      : undefined,
  })

  const problems = {
    image: imageProblem(form.image),
    name: serviceNameProblem(name, taken),
    port: portProblem(port),
    host: publicUrl ? hostProblem(host) : undefined,
    env: env.bad.length > 0,
    mountPath: volume && !MOUNT_PATH.test(extras.mountPath.trim()),
    volumeSize: volume && !quantity.test(extras.volumeSize.trim()),
    registry:
      registry && [extras.server, extras.username, extras.password].some((v) => v.trim() === ""),
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
      title: t("add.planImage", name),
      changes: createImageService(service(), domains),
    })
  }
  const editYaml = () => {
    handOff(project, toYaml(imageObjects(service(), domains)))
    onClose()
    void navigate({ to: "/projects/$project/create", params: { project } })
  }

  const moreOpen = tried && (problems.env || problems.mountPath || problems.volumeSize)

  return (
    <PlanDrawer
      step={planner}
      onClose={onClose}
      title={t("add.imageTitle")}
      description={t("add.imageBody", project)}
      footer={
        <div className="flex w-full flex-wrap items-center gap-2">
          <Button
            variant="ghost"
            size="sm"
            className="-ml-3"
            onClick={editYaml}
            disabled={form.image.trim() === ""}
          >
            {t("add.editYaml")}
          </Button>
          <div className="ml-auto flex gap-2">
            <Button variant="ghost" size="sm" onClick={onClose}>
              {t("edit.cancel")}
            </Button>
            <Button size="sm" onClick={review}>
              {t("edit.review")}
            </Button>
          </div>
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
          label={t("add.field.image")}
          hint={form.image.trim() && !hasTag(form.image) ? t("add.noTag") : undefined}
          errors={shown(problems.image, () => t(`add.problem.image.${problems.image ?? "empty"}`))}
        >
          <Input
            size="sm"
            mono
            value={form.image}
            placeholder="ghcr.io/acme/web:1.4"
            autoComplete="off"
            spellCheck={false}
            autoFocus
            onChange={(event) => set({ image: event.target.value })}
          />
        </FormField>
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
            <PortField
              value={portValue}
              onChange={setTypedPort}
              hint={
                note ? (
                  <PortNoteHint note={note} port={portValue} onPick={setTypedPort} />
                ) : (
                  t("add.portHint")
                )
              }
              error={shown(problems.port, () => t("add.problem.port.invalid"))}
            />
          ) : null}
        </div>
        <PublicUrlFields
          port={port}
          canPublish={options.publicUrl}
          on={form.publicUrl}
          onToggle={(publicUrl) => set({ publicUrl })}
          host={host}
          onHost={(typed) => {
            setHosted(true)
            set({ host: typed })
          }}
          domains={domains}
          error={shown(problems.host, () => t(`add.problem.host.${problems.host ?? "empty"}`))}
        />
        <Collapsible open={moreShown || moreOpen} onOpenChange={setMoreShown}>
          <CollapsibleTrigger variant="disclosure">
            <CaretRightIcon aria-hidden="true" />
            {t("add.more")}
          </CollapsibleTrigger>
          {moreShown || moreOpen ? null : (
            <p className="mt-1 type-meta text-muted-foreground">{t("add.moreSummary")}</p>
          )}
          <CollapsibleContent>
            <div className="flex flex-col gap-4 pt-4">
              <FormField
                label={t("add.variables")}
                hint={t("add.variablesHint")}
                errors={shown(problems.env, () => t("add.variablesBad", env.bad.join(", ")))}
              >
                <Textarea
                  mono
                  rows={4}
                  value={extras.env}
                  placeholder={"DATABASE_URL=postgres://…\nLOG_LEVEL=info"}
                  spellCheck={false}
                  onChange={(event) => more({ env: event.target.value })}
                />
              </FormField>
              {secrets ? (
                env.variables.length > 0 ? (
                  <Toggle
                    checked={form.secretVariables}
                    onChange={(secretVariables) => set({ secretVariables })}
                    label={t("add.variablesSecret", `${name || "…"}-env`)}
                  />
                ) : null
              ) : (
                <Note>{t("add.noSecrets")}</Note>
              )}
              {options.volume ? (
                <div className="flex flex-col gap-3">
                  <Toggle
                    checked={extras.volume}
                    onChange={(on) => more({ volume: on })}
                    label={t("add.volume")}
                  />
                  {volume ? (
                    <>
                      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
                        <FormField
                          label={t("add.volumePath")}
                          errors={shown(problems.mountPath, () => t("add.problem.volumePath"))}
                        >
                          <Input
                            size="sm"
                            mono
                            value={extras.mountPath}
                            onChange={(event) => more({ mountPath: event.target.value })}
                          />
                        </FormField>
                        <FormField
                          label={t("add.volumeSize")}
                          hint={growthWords(t, storage)}
                          errors={shown(problems.volumeSize, () => t("storage.problem.badSize"))}
                        >
                          <AmountInput
                            dimension="bytes"
                            value={extras.volumeSize}
                            onChange={(volumeSize) => more({ volumeSize })}
                          />
                        </FormField>
                      </div>
                      <Note>{t("add.volumeRecreate")}</Note>
                    </>
                  ) : null}
                </div>
              ) : (
                <Note>{t("add.noVolume")}</Note>
              )}
              <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
                <ImageSizeField size={form.size} onSize={(size) => set({ size })} />
                <FormField
                  label={t("add.replicas")}
                  hint={volume ? t("add.replicasFixed") : undefined}
                >
                  <Select
                    size="sm"
                    value={String(volume ? 1 : form.replicas)}
                    disabled={volume}
                    onValueChange={(replicas) => set({ replicas: Number(replicas) })}
                    items={Array.from({ length: MAX_REPLICAS }, (_, i) => ({
                      value: String(i + 1),
                      label: String(i + 1),
                    }))}
                  />
                </FormField>
              </div>
              {secrets ? (
                <div className="flex flex-col gap-3">
                  <Toggle
                    checked={extras.registry}
                    onChange={(on) => more({ registry: on })}
                    label={t("add.registry")}
                  />
                  {registry ? (
                    <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
                      <FormField
                        label={t("add.registryServer")}
                        errors={shown(problems.registry, () => t("add.problem.registry"))}
                      >
                        <Input
                          size="sm"
                          mono
                          value={extras.server}
                          placeholder="ghcr.io"
                          autoComplete="off"
                          onChange={(event) => more({ server: event.target.value })}
                        />
                      </FormField>
                      <FormField label={t("add.registryUser")}>
                        <Input
                          size="sm"
                          value={extras.username}
                          autoComplete="off"
                          onChange={(event) => more({ username: event.target.value })}
                        />
                      </FormField>
                      <FormField label={t("add.registryPassword")}>
                        <Input
                          size="sm"
                          type="password"
                          value={extras.password}
                          autoComplete="new-password"
                          onChange={(event) => more({ password: event.target.value })}
                        />
                      </FormField>
                    </div>
                  ) : null}
                </div>
              ) : null}
              <SecurityProfileField
                project={project}
                value={security}
                picked={picked}
                onChange={choose}
              />
            </div>
          </CollapsibleContent>
        </Collapsible>
      </form>
    </PlanDrawer>
  )
}

const registryLogin = ({ server, username, password }: Extras) =>
  [server, username, password].every((v) => v.trim() !== "")
    ? { server: server.trim(), username, password }
    : undefined

/** Strict when the image runs as a numeric user other than root, until the person chooses; and
 * why, while Zeabur chose. */
function useImageSecurity(inspection: { user?: string | null } | undefined) {
  const t = useT()
  const user = inspection?.user
  const [chosen, choose] = useState<Profile>()
  const security = chosen ?? imageProfile(user)
  let picked: string | undefined
  if (chosen === undefined && user != null)
    picked =
      security === "restricted"
        ? t("security.picked.restricted", user)
        : t("security.picked.baseline")
  return { security, picked, choose }
}
