import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import CaretDownIcon from "~icons/ph/caret-down"
import PlusIcon from "~icons/ph/plus"

import { creatableQuery, databasesQuery } from "@/api/queries"
import { useT } from "@/i18n"
import { openAdd } from "@/lib/adding"
import { engineName, engines, type Engine } from "@/model/databases"
import { rightsOf } from "@/model/image-service"
import { Button } from "@zeabur/design-system/components/button"
import {
  Menu,
  MenuContent,
  MenuItem,
  MenuLabel,
  MenuSeparator,
  MenuTrigger,
} from "@zeabur/design-system/components/menu"

const engineKinds: Record<Engine, [string, string]> = {
  postgresql: ["postgresql.cnpg.io", "clusters"],
  mongodb: ["psmdb.percona.com", "perconaservermongodbs"],
}

export interface AddChoices {
  image: boolean
  /** Offered engines; `installed` false points at Components instead. */
  databases: { engine: Engine; installed: boolean }[]
  yaml: boolean
  /** Nothing to offer: the person may only look. */
  none: boolean
}

/** What "New" offers in the project: a kind denied is left out, one unknown is offered and the
 * plan's dry run decides. */
export function useAddChoices(project: string | undefined): AddChoices | undefined {
  const creatable = useQuery({ ...creatableQuery(project ?? ""), enabled: !!project })
  const databases = useQuery({ ...databasesQuery(project ?? ""), enabled: !!project })
  if (!project || creatable.isPending) return undefined
  const rights = rightsOf(creatable.data)
  const operators = databases.data?.operators ?? []
  const image = rights("apps", "deployments") !== "denied"
  // An operator whose databases can't be read is left out: whether it is installed is unknown.
  const offered = engines
    .filter((engine) => rights(...engineKinds[engine]) !== "denied")
    .flatMap((engine) => {
      const state = operators.find((o) => o.engine === engine)?.state
      return state === "installed" || state === "not-installed"
        ? [{ engine, installed: state === "installed" }]
        : []
    })
  const yaml = (creatable.data?.items ?? []).some((k) => k.access !== "denied")
  return { image, databases: offered, yaml, none: !image && offered.length === 0 && !yaml }
}

/** "New": the one entry to add a service, a database or YAML to the project. */
export function AddMenu({ project, compact = false }: { project: string; compact?: boolean }) {
  const t = useT()
  const choices = useAddChoices(project)
  if (!choices || choices.none) return null
  const databases = choices.databases
  return (
    <Menu>
      <MenuTrigger
        render={
          compact ? (
            <Button variant="ghost" size="icon-xs" aria-label={t("add.inProject", project)} />
          ) : (
            <Button size="sm" />
          )
        }
      >
        <PlusIcon aria-hidden="true" data-icon={compact ? undefined : "inline-start"} />
        {compact ? null : (
          <>
            {t("add.button")}
            <CaretDownIcon aria-hidden="true" data-icon="inline-end" />
          </>
        )}
      </MenuTrigger>
      <MenuContent align={compact ? "start" : "end"} className="w-60">
        {choices.image ? (
          <MenuItem onClick={() => openAdd(project, "image")}>{t("add.image")}</MenuItem>
        ) : null}
        {databases.length > 0 ? (
          <>
            {choices.image ? <MenuSeparator /> : null}
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
