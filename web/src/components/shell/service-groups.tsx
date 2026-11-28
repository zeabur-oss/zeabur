import {
  DndContext,
  KeyboardSensor,
  MouseSensor,
  pointerWithin,
  rectIntersection,
  TouchSensor,
  useDraggable,
  useDroppable,
  useSensor,
  useSensors,
  type Announcements,
  type CollisionDetection,
  type DragEndEvent,
  type KeyboardCoordinateGetter,
} from "@dnd-kit/core"
import { Button } from "@zeabur-oss/design-system/components/button"
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@zeabur-oss/design-system/components/dialog"
import { Input } from "@zeabur-oss/design-system/components/input"
import { Tooltip } from "@zeabur-oss/design-system/components/tooltip"
import { cn } from "cn"
import { useState, type CSSProperties } from "react"
import DotsSixVerticalIcon from "~icons/ph/dots-six-vertical"
import FolderSimpleIcon from "~icons/ph/folder-simple"
import FoldersIcon from "~icons/ph/folders"
import PlusIcon from "~icons/ph/plus"

import { FormField } from "@/components/form-field"
import { GateTip, useGate } from "@/components/gate"
import { usePlanner } from "@/components/plan/planner"
import { ServiceIcon } from "@/components/service-icon"
import { useT } from "@/i18n"
import { serviceId, type Service } from "@/model/service"
import {
  groupMoves,
  groupSections,
  placementOf,
  validGroup,
  type GroupSection,
  type Placement,
} from "@/model/service-group"

/** The button next to "Services" that opens the group dialog. */
export function GroupButton({ project, services }: { project: string; services: Service[] }) {
  const t = useT()
  const gate = useGate(project, "changeServices")
  const [open, setOpen] = useState(false)
  const planner = usePlanner(project)
  const button = (
    <Button
      variant="ghost"
      size="icon-xs"
      aria-label={t("group.edit")}
      data-testid="group-services"
      onClick={() => setOpen(true)}
    >
      <FoldersIcon aria-hidden="true" />
    </Button>
  )
  return (
    <>
      {gate.open ? (
        <Tooltip content={t("group.edit")}>{button}</Tooltip>
      ) : (
        <GateTip gate={gate}>{button}</GateTip>
      )}
      {open ? (
        <GroupDialog
          services={services}
          onClose={() => setOpen(false)}
          onSave={(changes) => {
            setOpen(false)
            planner.plan({ title: t("group.edit"), changes })
          }}
        />
      ) : null}
      {planner.dialog}
    </>
  )
}

const sectionId = (name: string) => `group:${name}`

// The pointer decides while it is in a section; the keyboard moves the row onto one.
const collision: CollisionDetection = (args) => {
  const hits = pointerWithin(args)
  return hits.length > 0 ? hits : rectIntersection(args)
}

/** The arrow keys move the row to the top of the next or previous section. */
const nextSection: KeyboardCoordinateGetter = (event, { context }) => {
  const step = { ArrowDown: 1, ArrowRight: 1, ArrowUp: -1, ArrowLeft: -1 }[event.code]
  if (!step) return undefined
  event.preventDefault()
  const rects = [...context.droppableRects.entries()].toSorted(([, a], [, b]) => a.top - b.top)
  const at = rects.findIndex(([id]) => id === context.over?.id)
  const target = rects[Math.min(Math.max(at + step, 0), rects.length - 1)]?.[1]
  return target ? { x: target.left + 8, y: target.top + 8 } : undefined
}

function GroupDialog({
  services,
  onClose,
  onSave,
}: {
  services: Service[]
  onClose: () => void
  onSave: (changes: ReturnType<typeof groupMoves>) => void
}) {
  const t = useT()
  const [placement, setPlacement] = useState<Placement>(() => placementOf(services))
  const [added, setAdded] = useState<string[]>([])
  const sections = groupSections(services, placement, added)
  const moves = groupMoves(services, placement)
  const sensors = useSensors(
    useSensor(MouseSensor, { activationConstraint: { distance: 4 } }),
    useSensor(TouchSensor, {
      activationConstraint: { delay: 200, tolerance: 6 },
    }),
    useSensor(KeyboardSensor, { coordinateGetter: nextSection }),
  )
  const label = (name: string) => name || t("project.ungrouped")
  const nameOf = (id: unknown) => {
    const service = services.find((s) => serviceId(s) === id)
    return service ? (service.displayName ?? service.name) : ""
  }
  const groupOf = (id: unknown) => (typeof id === "string" ? id.slice("group:".length) : "")
  const announcements: Announcements = {
    onDragStart: ({ active }) => t("group.pickedUp", nameOf(active.id)),
    onDragOver: ({ active, over }) =>
      over
        ? t("group.over", nameOf(active.id), label(groupOf(over.id)))
        : t("group.notOver", nameOf(active.id)),
    onDragEnd: ({ active, over }) =>
      over
        ? t("group.dropped", nameOf(active.id), label(groupOf(over.id)))
        : t("group.stays", nameOf(active.id)),
    onDragCancel: ({ active }) => t("group.stays", nameOf(active.id)),
  }
  const drop = ({ active, over }: DragEndEvent) => {
    if (!over || typeof active.id !== "string") return
    setPlacement({ ...placement, [active.id]: groupOf(over.id) })
  }
  return (
    <Dialog open onOpenChange={(next) => (next ? undefined : onClose())}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader closeLabel={t("common.close")}>
          <DialogTitle>{t("group.edit")}</DialogTitle>
          <DialogDescription>{t("group.hint")}</DialogDescription>
        </DialogHeader>
        <DialogBody>
          <div className="flex flex-col gap-4">
            <DndContext
              sensors={sensors}
              collisionDetection={collision}
              accessibility={{
                announcements,
                screenReaderInstructions: { draggable: t("group.dragHint") },
              }}
              onDragEnd={drop}
            >
              {sections.map((section) => (
                <Section key={section.name} section={section} label={label(section.name)} />
              ))}
            </DndContext>
            <NewGroup
              taken={sections.map((s) => s.name)}
              onAdd={(name) => setAdded([...added, name])}
            />
          </div>
        </DialogBody>
        <DialogFooter>
          <Button variant="ghost" size="sm" onClick={onClose}>
            {t("edit.cancel")}
          </Button>
          <Button size="sm" disabled={moves.length === 0} onClick={() => onSave(moves)}>
            {t("edit.review")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function Section({ section, label }: { section: GroupSection; label: string }) {
  const t = useT()
  const { setNodeRef, isOver } = useDroppable({ id: sectionId(section.name) })
  return (
    <section
      ref={setNodeRef}
      aria-label={label}
      className={cn(
        "flex flex-col gap-1 rounded-xl border border-v2-border p-2 transition-colors",
        isOver && "border-v2-border-strong bg-v2-surface-hover",
      )}
    >
      <h3 className="flex items-center gap-2 px-1 py-1 type-label">
        <FolderSimpleIcon aria-hidden="true" className="size-4 shrink-0" />
        <span className="min-w-0 flex-1 truncate" translate={section.name ? "no" : undefined}>
          {label}
        </span>
        <span className="type-nano text-tertiary">{section.services.length}</span>
      </h3>
      {section.services.length === 0 ? (
        <p className="px-1 py-2 text-xs text-tertiary">{t("group.empty")}</p>
      ) : (
        section.services.map((service) => <Row key={serviceId(service)} service={service} />)
      )}
    </section>
  )
}

function Row({ service }: { service: Service }) {
  const t = useT()
  const managed = service.provenance.tool ?? undefined
  const { attributes, listeners, setNodeRef, transform, isDragging } = useDraggable({
    id: serviceId(service),
    disabled: !!managed,
  })
  return (
    <div
      ref={setNodeRef}
      {...attributes}
      {...listeners}
      style={
        transform
          ? ({
              "--drag-x": `${transform.x}px`,
              "--drag-y": `${transform.y}px`,
            } as CSSProperties)
          : undefined
      }
      className={cn(
        "flex h-10 items-center gap-2 rounded-lg border border-v2-border bg-card px-2 type-body outline-none focus-visible:ring-2 focus-visible:ring-ring",
        managed ? "text-tertiary" : "cursor-grab touch-manipulation",
        transform && "translate-x-(--drag-x) translate-y-(--drag-y)",
        isDragging && "relative z-10 cursor-grabbing shadow-lg",
      )}
    >
      <DotsSixVerticalIcon
        aria-hidden="true"
        className={cn("size-4 shrink-0 text-tertiary", managed && "invisible")}
      />
      <ServiceIcon service={service} size="sm" />
      <span className="min-w-0 flex-1 truncate" translate="no">
        {service.displayName ?? service.name}
      </span>
      {managed ? (
        <span className="shrink-0 text-xs text-tertiary">{t("common.managedBy", managed)}</span>
      ) : null}
    </div>
  )
}

function NewGroup({ taken, onAdd }: { taken: string[]; onAdd: (name: string) => void }) {
  const t = useT()
  const [typed, setTyped] = useState<string>()
  if (typed === undefined)
    return (
      <Button variant="outline" size="sm" className="self-start" onClick={() => setTyped("")}>
        <PlusIcon aria-hidden="true" data-icon="inline-start" />
        {t("group.new")}
      </Button>
    )
  const name = typed.trim()
  const invalid = name !== "" && !validGroup(name)
  const add = () => {
    if (!name || invalid) return
    if (!taken.includes(name)) onAdd(name)
    setTyped(undefined)
  }
  return (
    <form
      className="flex items-start gap-2"
      onSubmit={(event) => {
        event.preventDefault()
        add()
      }}
    >
      <FormField
        label={t("group.name")}
        hideLabel
        errors={invalid ? [t("group.invalid")] : []}
        className="min-w-0 flex-1"
      >
        <Input
          size="sm"
          autoFocus
          value={typed}
          placeholder={t("group.placeholder")}
          spellCheck={false}
          autoComplete="off"
          onChange={(event) => setTyped(event.target.value)}
          onKeyDown={(event) => {
            if (event.key !== "Escape") return
            event.stopPropagation()
            setTyped(undefined)
          }}
        />
      </FormField>
      <Button type="submit" size="sm" variant="outline" disabled={!name || invalid}>
        {t("group.add")}
      </Button>
    </form>
  )
}
