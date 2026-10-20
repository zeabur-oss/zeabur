// The project switcher beside the project's name in its sidebar: a filterable list of the other
// projects.
import { useQuery } from "@tanstack/react-query"
import { useNavigate } from "@tanstack/react-router"
import { useState } from "react"
import CaretUpDownIcon from "~icons/ph/caret-up-down"
import CheckIcon from "~icons/ph/check"
import MagnifyingGlassIcon from "~icons/ph/magnifying-glass"

import { projectsQuery } from "@/api/queries"
import { ProjectGlyph } from "@/components/shell/project-glyph"
import { Popover, PopoverContent, PopoverTrigger } from "@zeabur/design-system/components/popover"
import { useT } from "@/i18n"
import { cn } from "@/lib/utils"
import { projectTitle } from "@/model/project"

const triggerClass = "inline-flex shrink-0 items-center justify-center"

const optionClass =
  "flex w-full items-center gap-2 rounded-lg px-2 text-left outline-none hover:bg-muted focus-visible:bg-muted"

export function ProjectSwitcher({ current }: { current: string }) {
  const t = useT()
  const navigate = useNavigate()
  const projects = useQuery(projectsQuery)
  const [open, setOpen] = useState(false)
  const [filter, setFilter] = useState("")
  const items = (projects.data?.items ?? []).filter((p) =>
    [p.name, p.displayName ?? ""].some((n) => n.toLowerCase().includes(filter.toLowerCase())),
  )
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger
        variant="ghost"
        className={cn(triggerClass, "size-7")}
        aria-label={t("shell.switchProject")}
        title={t("shell.switchProject")}
      >
        <CaretUpDownIcon className="size-3.5" aria-hidden="true" />
      </PopoverTrigger>
      <PopoverContent className="w-64">
        <PickerInput value={filter} onChange={setFilter} label={t("shell.findProject")} />
        <ul className="flex max-h-72 flex-col overflow-y-auto py-1">
          {items.map((p) => (
            <li key={p.name}>
              <button
                type="button"
                onClick={() => {
                  setOpen(false)
                  void navigate({ to: "/projects/$project", params: { project: p.name } })
                }}
                className={cn(optionClass, "h-8")}
              >
                <ProjectGlyph icon={p.icon} className="size-3.5" />
                <span className="truncate" translate="no">
                  {projectTitle(p, p.name)}
                </span>
                {p.adopted ? null : (
                  <span className="type-meta text-muted-foreground">
                    {t("projects.namespaceOnly")}
                  </span>
                )}
                {p.name === current ? (
                  <CheckIcon className="ml-auto size-3.5" aria-hidden="true" />
                ) : null}
              </button>
            </li>
          ))}
        </ul>
      </PopoverContent>
    </Popover>
  )
}

function PickerInput({
  value,
  onChange,
  label,
}: {
  value: string
  onChange: (value: string) => void
  label: string
}) {
  return (
    <div className="flex items-center gap-2 border-b px-2 pb-1">
      <MagnifyingGlassIcon className="size-3.5 text-muted-foreground" aria-hidden="true" />
      <input
        value={value}
        onChange={(event) => onChange(event.target.value)}
        aria-label={label}
        className="h-8 w-full bg-transparent text-sm outline-none placeholder:text-muted-foreground"
      />
    </div>
  )
}
