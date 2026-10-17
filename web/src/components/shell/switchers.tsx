// The ▾ switchers next to a crumb: the crumb itself is a link to the project or service; this
// separate button opens a filterable list of the others.
import { useQuery } from "@tanstack/react-query"
import { useNavigate } from "@tanstack/react-router"
import { useState } from "react"
import CaretDownIcon from "~icons/ph/caret-down"
import CaretUpDownIcon from "~icons/ph/caret-up-down"
import CheckIcon from "~icons/ph/check"
import MagnifyingGlassIcon from "~icons/ph/magnifying-glass"

import { projectQuery, projectsQuery } from "@/api/queries"
import { ProjectGlyph } from "@/components/shell/project-glyph"
import { ServiceIcon } from "@/components/service-icon"
import { StatusDot } from "@/components/status"
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover"
import { useT } from "@/i18n"
import { serviceLink, usePlace } from "@/lib/place"
import { cn } from "@/lib/utils"
import { projectTitle } from "@/model/project"
import { serviceId, statusOf } from "@/model/service"

const triggerClass = "inline-flex shrink-0 items-center justify-center"

const optionClass =
  "flex w-full items-center gap-2 rounded-lg px-2 text-left outline-none hover:bg-muted focus-visible:bg-muted"

/** `caret` in the breadcrumb, `updown` in the project column's header. */
export function ProjectSwitcher({
  current,
  trigger = "caret",
}: {
  current: string
  trigger?: "caret" | "updown"
}) {
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
        className={cn(triggerClass, trigger === "caret" ? "size-6" : "size-7")}
        aria-label={t("shell.switchProject")}
        title={t("shell.switchProject")}
      >
        {trigger === "caret" ? (
          <CaretDownIcon className="size-3.5" aria-hidden="true" />
        ) : (
          <CaretUpDownIcon className="size-4" aria-hidden="true" />
        )}
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

export function ServiceSwitcher({ project, current }: { project: string; current: string }) {
  const t = useT()
  const place = usePlace()
  const navigate = useNavigate()
  const view = useQuery(projectQuery(project))
  const [open, setOpen] = useState(false)
  const [filter, setFilter] = useState("")
  const services = (view.data?.services ?? []).filter((s) =>
    s.name.toLowerCase().includes(filter.toLowerCase()),
  )
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger
        variant="ghost"
        className={cn(triggerClass, "size-6")}
        aria-label={t("shell.switchService")}
        title={t("shell.switchService")}
      >
        <CaretDownIcon className="size-3.5" aria-hidden="true" />
      </PopoverTrigger>
      <PopoverContent className="w-72">
        <PickerInput value={filter} onChange={setFilter} label={t("shell.findService")} />
        <ul className="flex max-h-80 flex-col overflow-y-auto py-1">
          {services.map((s) => (
            <li key={serviceId(s)}>
              <button
                type="button"
                onClick={() => {
                  setOpen(false)
                  void navigate(serviceLink(project, s, { tab: place.tab }))
                }}
                className={cn(optionClass, "h-9")}
              >
                <ServiceIcon service={s} size="sm" />
                <span className="truncate" translate="no">
                  {s.name}
                </span>
                <span className="type-meta text-muted-foreground">{s.kind}</span>
                <span className="ml-auto flex items-center gap-2">
                  {serviceId(s) === current ? (
                    <CheckIcon className="size-3.5" aria-hidden="true" />
                  ) : null}
                  <StatusDot health={statusOf(s).health} />
                </span>
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
