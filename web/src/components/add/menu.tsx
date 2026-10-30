import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import { cloneElement, Fragment, useState } from "react"
import CaretDownIcon from "~icons/ph/caret-down"
import PlusIcon from "~icons/ph/plus"

import type { Schemas } from "@/api/client"
import {
  buildsQuery,
  capabilitiesQuery,
  clusterQuery,
  creatableQuery,
  databasesQuery,
  objectStorageQuery,
} from "@/api/queries"
import { GatedMenuItem, GateTip, useGates } from "@/components/gate"
import { LazyInstallerSheet } from "@/components/lazy-installer-sheet"
import { useT, type Translate } from "@/i18n"
import { openAdd, type AddWhat } from "@/lib/adding"
import { engineName, engineOperator, engines, type Engine } from "@/model/databases"
import { gitNeeds } from "@/model/git-service"
import { needed, type Needed } from "@/model/installer"
import type { Gate } from "@/model/rights"
import { Button } from "@zeabur/design-system/components/button"
import {
  Menu,
  MenuContent,
  MenuItem,
  MenuLabel,
  MenuSeparator,
  MenuTrigger,
} from "@zeabur/design-system/components/menu"

export interface AddChoices {
  image: boolean
  /** Offered engines; `installed` false offers the operator's install steps instead. */
  databases: { engine: Engine; installed: boolean; operator: Needed }[]
  /** Object storage, when the person may create it and whether its operator is installed is
   * known. */
  objectStorage?: { installed: boolean; operator: Needed }
  /** "Deploy from Git": open, or closed by the person's role or by what the cluster lacks. */
  git: { open: boolean; gate: Gate; install: Needed[]; registrySetting: boolean }
  yaml: boolean
  /** Nothing to offer: the person may only look. */
  none: boolean
  /** Why a service, or a database, cannot be added. */
  gates: { image: Gate; database: Gate }
}

type Capabilities = Schemas.Capability[] | undefined

/** Engines whose operator is known to be installed or not. An operator whose databases can't be
 * read is left out. */
function knownEngines(
  operators: { engine: Engine; state: string }[] = [],
  capabilities: Capabilities,
) {
  return engines.flatMap((engine) => {
    const state = operators.find((o) => o.engine === engine)?.state
    return state === "installed" || state === "not-installed"
      ? [
          {
            engine,
            installed: state === "installed",
            operator: needed(engineOperator[engine], capabilities),
          },
        ]
      : []
  })
}

/** Object storage, when the role allows it and whether the operator is installed is known. */
function useObjectStorageChoice(project: string | undefined, capabilities: Capabilities) {
  const gate = useGates(project)("changeObjectStorage")
  const state = useQuery({ ...objectStorageQuery(project ?? ""), enabled: !!project }).data?.state
  if (!gate.open || (state !== "installed" && state !== "not-installed")) return undefined
  return { installed: state === "installed", operator: needed("object-storage", capabilities) }
}

/** "Deploy from Git": open, or closed by the person's role or by what the cluster lacks. */
function useGitChoice(project: string | undefined, capabilities: Capabilities): AddChoices["git"] {
  const builds = useQuery({
    ...buildsQuery(project ?? ""),
    enabled: !!project,
    refetchInterval: false,
  })
  const cluster = useQuery(clusterQuery).data
  const gate = useGates(project)("deployFromGit")
  const needs = gitNeeds({
    builds: builds.data?.state,
    capabilities,
    registry: cluster && (cluster.builds.registry ?? ""),
  })
  return gitChoice(gate, needs, capabilities)
}

function gitChoice(
  gate: Gate,
  needs: ReturnType<typeof gitNeeds>,
  capabilities: Capabilities,
): AddChoices["git"] {
  if (!needs) return { open: gate.open, gate, install: [], registrySetting: false }
  return {
    open: false,
    gate,
    install: needs.install.map((id) => needed(id, capabilities)),
    registrySetting: needs.registrySetting,
  }
}

/** What "New" offers in the project: a kind the person's role cannot add is closed, one unknown
 * is offered and the plan's dry run decides. */
export function useAddChoices(project: string | undefined): AddChoices | undefined {
  const creatable = useQuery({ ...creatableQuery(project ?? ""), enabled: !!project })
  const databases = useQuery({ ...databasesQuery(project ?? ""), enabled: !!project })
  // Readable by cluster administrators only; for anyone else what is installed stays unknown.
  const capabilities = useQuery({ ...capabilitiesQuery, retry: false }).data?.items
  const gates = useGates(project)
  const store = useObjectStorageChoice(project, capabilities)
  const git = useGitChoice(project, capabilities)
  if (!project || creatable.isPending) return undefined
  const imageGate = gates("deploy")
  const databaseGate = gates("changeDatabases")
  const offered = databaseGate.open ? knownEngines(databases.data?.operators, capabilities) : []
  const yaml = (creatable.data?.items ?? []).some((k) => k.access !== "denied")
  return {
    image: imageGate.open,
    git,
    databases: offered,
    objectStorage: store,
    yaml,
    none: !imageGate.open && offered.length === 0 && !store && !yaml,
    gates: { image: imageGate, database: databaseGate },
  }
}

/** The install steps a closed item opens. */
export interface Steps {
  components: Needed[]
  reason?: string
  note?: string
}

/** One thing "New" offers, shared by the menu and the empty project's tiles. */
export interface AddEntry {
  what: AddWhat
  group: "service" | "database" | "storage"
  label: string
  /** The role's verdict; a closed entry shows why. */
  gate: Gate
  /** Set when the cluster lacks something: what, said under the label. */
  why?: string
  /** The install steps that open it, when Zeabur knows them. */
  steps?: Steps
}

export const entryTestid = (what: AddWhat) =>
  `add-${what === "objectStorage" ? "object-storage" : what}`

function gitEntry(git: AddChoices["git"], what: "git" | "upload", t: Translate): AddEntry {
  const entry: AddEntry = { what, group: "service", label: t(`add.${what}`), gate: git.gate }
  if (git.open || !git.gate.open) return entry
  const note = git.registrySetting ? t("add.gitMissing.registrySetting") : undefined
  if (git.install.length === 0) return { ...entry, why: note ?? "" }
  const names = t(
    "common.list",
    git.install.map((n) => n.name),
  )
  return {
    ...entry,
    why: t("add.installFirst", names),
    steps: { components: git.install, reason: t("add.gitNeedsWhy"), note },
  }
}

function operatorEntry(
  what: AddWhat,
  group: AddEntry["group"],
  label: string,
  { installed, operator }: { installed: boolean; operator: Needed },
  t: Translate,
): AddEntry {
  const entry: AddEntry = { what, group, label, gate: { open: true } }
  if (installed) return entry
  return { ...entry, why: t("add.installFirst", operator.name), steps: { components: [operator] } }
}

/** Everything "New" offers, in the menu's order. */
export function addEntries(choices: AddChoices, t: Translate): AddEntry[] {
  const image = choices.gates.image
  const store = choices.objectStorage
  return [
    { what: "image", group: "service", label: t("add.image"), gate: image },
    gitEntry(choices.git, "git", t),
    gitEntry(choices.git, "upload", t),
    { what: "compose", group: "service", label: t("add.compose"), gate: image },
    { what: "template", group: "service", label: t("add.template"), gate: image },
    ...choices.databases.map((d) =>
      operatorEntry(d.engine, "database", engineName(d.engine), d, t),
    ),
    ...(store ? [operatorEntry("objectStorage", "storage", t("s3.title"), store, t)] : []),
  ]
}

/** "New": the one entry to add a service, a database or YAML to the project. */
export function AddMenu({ project, compact = false }: { project: string; compact?: boolean }) {
  const t = useT()
  const choices = useAddChoices(project)
  const [steps, setSteps] = useState<Steps>()
  if (!choices) return null
  const trigger = compact ? (
    <Button variant="ghost" size="icon-xs" aria-label={t("add.inProject", project)} />
  ) : (
    <Button size="sm" data-testid="add" />
  )
  const face = (
    <>
      <PlusIcon aria-hidden="true" data-icon={compact ? undefined : "inline-start"} />
      {compact ? null : (
        <>
          {t("add.button")}
          <CaretDownIcon aria-hidden="true" data-icon="inline-end" />
        </>
      )}
    </>
  )
  // A Viewer sees what Developers use to add things, closed, with the role that opens it.
  if (choices.none)
    return <GateTip gate={choices.gates.image}>{cloneElement(trigger, {}, face)}</GateTip>
  const entries = addEntries(choices, t)
  return (
    <>
      <Menu>
        <MenuTrigger render={trigger}>{face}</MenuTrigger>
        <MenuContent align={compact ? "start" : "end"} className="w-60">
          {entries.map((entry, i) => (
            <Fragment key={entry.what}>
              {i > 0 && entries[i - 1]?.group !== entry.group ? <MenuSeparator /> : null}
              <EntryItem
                entry={entry}
                onOpen={() => openAdd(project, entry.what)}
                onSteps={setSteps}
              />
            </Fragment>
          ))}
          {!choices.gates.database.open ? (
            <>
              <MenuSeparator />
              <GatedMenuItem gate={choices.gates.database}>{t("add.database")}</GatedMenuItem>
            </>
          ) : null}
          {choices.yaml ? (
            <>
              <MenuSeparator />
              <MenuLabel>{t("add.advanced")}</MenuLabel>
              <MenuItem render={<Link to="/projects/$project/create" params={{ project }} />}>
                {t("add.yaml")}
              </MenuItem>
            </>
          ) : null}
        </MenuContent>
      </Menu>
      {steps ? <LazyInstallerSheet {...steps} onClose={() => setSteps(undefined)} /> : null}
    </>
  )
}

/** An entry: open, closed by the role, or what the cluster lacks with its install steps. */
function EntryItem({
  entry,
  onOpen,
  onSteps,
}: {
  entry: AddEntry
  onOpen: () => void
  onSteps: (steps: Steps) => void
}) {
  const { steps } = entry
  if (entry.why === undefined)
    return (
      <GatedMenuItem gate={entry.gate} testid={entryTestid(entry.what)} onClick={onOpen}>
        {entry.label}
      </GatedMenuItem>
    )
  return (
    <MenuItem
      data-testid={entryTestid(entry.what)}
      disabled={!steps}
      onClick={steps ? () => onSteps(steps) : undefined}
    >
      <span className="flex min-w-0 flex-col gap-0.5">
        <span className="text-muted-foreground">{entry.label}</span>
        <span className="type-meta text-muted-foreground">{entry.why}</span>
      </span>
    </MenuItem>
  )
}
