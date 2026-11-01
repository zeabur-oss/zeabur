import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import { Fragment, useState, type ComponentType, type SVGProps } from "react"
import ArchiveIcon from "~icons/ph/archive"
import CaretRightIcon from "~icons/ph/caret-right"
import DatabaseIcon from "~icons/ph/database"
import GitBranchIcon from "~icons/ph/git-branch"
import PackageIcon from "~icons/ph/package"
import SquaresFourIcon from "~icons/ph/squares-four"
import StackIcon from "~icons/ph/stack"
import UploadSimpleIcon from "~icons/ph/upload-simple"

import { objectStorageQuery } from "@/api/queries"
import {
  addEntries,
  entryTestid,
  useAddChoices,
  type AddEntry,
  type Steps,
} from "@/components/add/menu"
import { CopyText } from "@/components/copy-text"
import { useDatabases } from "@/components/databases"
import { LazyInstallerSheet } from "@/components/lazy-installer-sheet"
import { useT } from "@/i18n"
import { openAdd, type AddWhat } from "@/lib/adding"
import { cn } from "@/lib/utils"
import { Databases, ObjectStorageSection } from "@/routes/project/side"
import { ActionTile } from "@zeabur-oss/design-system/components/action-tile"
import { buttonVariants } from "@zeabur-oss/design-system/components/button"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@zeabur-oss/design-system/components/collapsible"
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@zeabur-oss/design-system/components/empty"
import { Illustration } from "@zeabur-oss/design-system/components/illustration"
import { PageColumn } from "@zeabur-oss/design-system/components/page-column"

const icons: Partial<Record<AddWhat, ComponentType<SVGProps<SVGSVGElement>>>> = {
  image: PackageIcon,
  git: GitBranchIcon,
  upload: UploadSimpleIcon,
  compose: StackIcon,
  template: SquaresFourIcon,
  objectStorage: ArchiveIcon,
}

/** An empty project: the same choices as "New", as tiles. */
export function NextStep({ project }: { project: string }) {
  const t = useT()
  const choices = useAddChoices(project)
  const [steps, setSteps] = useState<Steps>()
  const databases = useDatabases(project).data?.items.length ?? 0
  const stores = useQuery(objectStorageQuery(project)).data?.items.length ?? 0
  const entries = choices && !choices.none ? addEntries(choices, t).filter((e) => e.gate.open) : []
  const services = entries.filter((e) => e.group === "service")
  const data = entries.filter((e) => e.group !== "service")
  return (
    <PageColumn gap="md" className="mb-12">
      <Empty variant="outline">
        <EmptyHeader>
          <EmptyMedia>
            <Illustration kind="empty" seed="services" />
          </EmptyMedia>
          <EmptyTitle>{t("project.next.title")}</EmptyTitle>
          <EmptyDescription>
            {choices?.none ? t("add.viewer", project) : t("add.emptyBody")}
          </EmptyDescription>
        </EmptyHeader>
        <EmptyContent className="w-full max-w-2xl">
          <div className="flex w-full flex-col gap-6 text-left">
            {services.length ? (
              <TileGroup
                title={t("add.group.services")}
                entries={services}
                project={project}
                onSteps={setSteps}
              />
            ) : null}
            {data.length ? (
              <TileGroup
                title={t("add.group.data")}
                hint={t("add.group.dataHint")}
                entries={data}
                project={project}
                onSteps={setSteps}
              />
            ) : null}
            <OtherWays project={project} yaml={!!choices?.yaml} />
          </div>
        </EmptyContent>
      </Empty>
      {databases ? <Databases project={project} /> : null}
      {stores ? <ObjectStorageSection project={project} /> : null}
      {steps ? <LazyInstallerSheet {...steps} onClose={() => setSteps(undefined)} /> : null}
    </PageColumn>
  )
}

function TileGroup({
  title,
  hint,
  entries,
  project,
  onSteps,
}: {
  title: string
  hint?: string
  entries: AddEntry[]
  project: string
  onSteps: (steps: Steps) => void
}) {
  return (
    <section className="flex flex-col gap-3">
      <div className="flex flex-col gap-0.5">
        <h3 className="type-meta font-medium text-foreground">{title}</h3>
        {hint ? <p className="type-meta text-muted-foreground">{hint}</p> : null}
      </div>
      <div className="grid grid-cols-2 gap-3 sm:grid-cols-3">
        {entries.map((entry) => (
          <EntryTile key={entry.what} entry={entry} onClick={tileAction(entry, project, onSteps)} />
        ))}
      </div>
    </section>
  )
}

function tileAction(entry: AddEntry, project: string, onSteps: (steps: Steps) => void) {
  const { steps } = entry
  if (entry.why === undefined) return () => openAdd(project, entry.what)
  return steps ? () => onSteps(steps) : undefined
}

function EntryTile({ entry, onClick }: { entry: AddEntry; onClick?: () => void }) {
  const t = useT()
  const Icon = icons[entry.what] ?? DatabaseIcon
  const body = entry.why ?? t(`add.card.${entry.what}`)
  return (
    <ActionTile
      data-testid={`${entryTestid(entry.what)}-tile`}
      onClick={onClick}
      disabled={!onClick}
    >
      <Icon aria-hidden="true" />
      <span className="flex flex-col gap-1">
        <span className={cn(entry.why !== undefined && "text-muted-foreground")}>
          {entry.label}
        </span>
        {body ? <span className="type-meta font-normal text-muted-foreground">{body}</span> : null}
      </span>
    </ActionTile>
  )
}

/** YAML and kubectl, for people who already know them. */
function OtherWays({ project, yaml }: { project: string; yaml: boolean }) {
  const t = useT()
  const command = `kubectl create deployment hello --image=nginx --port=80 -n ${project}`
  return (
    <Collapsible className="flex w-full flex-col items-center">
      <CollapsibleTrigger variant="disclosure">
        <CaretRightIcon aria-hidden="true" />
        {t("add.otherWays")}
      </CollapsibleTrigger>
      <CollapsibleContent className="w-full">
        <div className="flex w-full flex-col gap-1.5 pt-3">
          {yaml ? (
            <Link
              to="/projects/$project/create"
              params={{ project }}
              className={cn(buttonVariants({ variant: "outline", size: "sm" }), "mb-3 self-start")}
            >
              {t("add.yaml")}
            </Link>
          ) : null}
          <span className="type-meta text-muted-foreground">{t("project.next.body")}</span>
          <span className="type-meta text-muted-foreground">{t("project.next.command")}</span>
          <div className="flex flex-wrap items-center gap-x-2 gap-y-1 rounded-lg border bg-muted/40 py-1.5 pr-1.5 pl-3">
            <code className="min-w-48 flex-1 type-mono break-words" translate="no">
              {command.split(" ").map((word, i) => (
                <Fragment key={i}>
                  {i > 0 ? " " : null}
                  <span className="whitespace-nowrap">{word}</span>
                </Fragment>
              ))}
            </code>
            <div className="ml-auto flex items-center">
              <CopyText value={() => command} label={t("common.copy")} />
            </div>
          </div>
          <span className="type-meta text-muted-foreground">{t("project.next.after")}</span>
          <Link
            to="/projects/$project/objects"
            params={{ project }}
            className={cn(buttonVariants({ variant: "outline", size: "sm" }), "mt-2 self-start")}
          >
            {t("project.seeObjects")}
          </Link>
        </div>
      </CollapsibleContent>
    </Collapsible>
  )
}
