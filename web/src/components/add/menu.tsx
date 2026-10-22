import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import { cloneElement } from "react"
import CaretDownIcon from "~icons/ph/caret-down"
import PlusIcon from "~icons/ph/plus"

import { creatableQuery, databasesQuery } from "@/api/queries"
import { GatedMenuItem, GateTip, useGates } from "@/components/gate"
import { useT } from "@/i18n"
import { openAdd } from "@/lib/adding"
import { engineName, engines, type Engine } from "@/model/databases"
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
  /** Offered engines; `installed` false points at Components instead. */
  databases: { engine: Engine; installed: boolean }[]
  yaml: boolean
  /** Nothing to offer: the person may only look. */
  none: boolean
  /** Why a service, or a database, cannot be added. */
  gates: { image: Gate; database: Gate }
}

/** What "New" offers in the project: a kind the person's role cannot add is closed, one unknown
 * is offered and the plan's dry run decides. */
export function useAddChoices(project: string | undefined): AddChoices | undefined {
  const creatable = useQuery({ ...creatableQuery(project ?? ""), enabled: !!project })
  const databases = useQuery({ ...databasesQuery(project ?? ""), enabled: !!project })
  const gates = useGates(project)
  if (!project || creatable.isPending) return undefined
  const operators = databases.data?.operators ?? []
  const imageGate = gates("deploy")
  const databaseGate = gates("changeDatabases")
  // An operator whose databases can't be read is left out: whether it is installed is unknown.
  const known = engines.flatMap((engine) => {
    const state = operators.find((o) => o.engine === engine)?.state
    return state === "installed" || state === "not-installed"
      ? [{ engine, installed: state === "installed" }]
      : []
  })
  const offered = databaseGate.open ? known : []
  const yaml = (creatable.data?.items ?? []).some((k) => k.access !== "denied")
  return {
    image: imageGate.open,
    databases: offered,
    yaml,
    none: !imageGate.open && offered.length === 0 && !yaml,
    gates: { image: imageGate, database: databaseGate },
  }
}

/** "New": the one entry to add a service, a database or YAML to the project. */
export function AddMenu({ project, compact = false }: { project: string; compact?: boolean }) {
  const t = useT()
  const choices = useAddChoices(project)
  if (!choices) return null
  const trigger = compact ? (
    <Button variant="ghost" size="icon-xs" aria-label={t("add.inProject", project)} />
  ) : (
    <Button size="sm" />
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
  const databases = choices.databases
  return (
    <Menu>
      <MenuTrigger render={trigger}>{face}</MenuTrigger>
      <MenuContent align={compact ? "start" : "end"} className="w-60">
        <GatedMenuItem gate={choices.gates.image} onClick={() => openAdd(project, "image")}>
          {t("add.image")}
        </GatedMenuItem>
        {!choices.gates.database.open ? (
          <>
            <MenuSeparator />
            <GatedMenuItem gate={choices.gates.database}>{t("add.database")}</GatedMenuItem>
          </>
        ) : null}
        {databases.length > 0 ? (
          <>
            <MenuSeparator />
            {databases.map(({ engine, installed }) =>
              installed ? (
                <MenuItem key={engine} onClick={() => openAdd(project, engine)}>
                  {engineName(engine)}
                </MenuItem>
              ) : (
                <MenuItem key={engine} render={<Link to="/cluster/components" />}>
                  <span className="flex min-w-0 flex-col">
                    <span className="text-muted-foreground">{engineName(engine)}</span>
                    <span className="type-meta text-muted-foreground">
                      {t("add.needsComponent")}
                    </span>
                  </span>
                </MenuItem>
              ),
            )}
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
  )
}
