import type { Menu as MenuPrimitive } from "@base-ui/react/menu"
import { cn } from "cn"

import {
  PendingRoot,
  PendingTrigger,
  useLoadedPopups,
  useOpen,
  usePopups,
  type RootProps,
  type TriggerProps,
} from "@/components/ui/popup"

// Base UI's menu, whose code loads after the first page (popup.tsx).
function Menu({ open, defaultOpen, onOpenChange, ...props }: RootProps<MenuPrimitive.Root.Props>) {
  const state = useOpen(open, defaultOpen, onOpenChange)
  if (!state.parts) return <PendingRoot setOpen={state.setOpen}>{props.children}</PendingRoot>
  return (
    <state.parts.Menu.Root
      data-slot="menu"
      {...props}
      open={state.open}
      onOpenChange={(next) => state.setOpen(next)}
    />
  )
}

function MenuTrigger({ className, ...props }: TriggerProps<MenuPrimitive.Trigger.Props>) {
  const parts = usePopups()
  if (!parts)
    return (
      <PendingTrigger haspopup="menu" data-slot="menu-trigger" className={className} {...props} />
    )
  return <parts.Menu.Trigger data-slot="menu-trigger" className={className} {...props} />
}

function MenuContent({
  className,
  align = "end",
  ...props
}: MenuPrimitive.Popup.Props & Pick<MenuPrimitive.Positioner.Props, "align">) {
  const parts = usePopups()
  // Closed until the code is there.
  if (!parts) return null
  const { Menu: M } = parts
  return (
    <M.Portal>
      <M.Positioner className="isolate z-50" align={align} sideOffset={6}>
        <M.Popup
          data-slot="menu-content"
          className={cn(
            "min-w-48 origin-(--transform-origin) rounded-xl bg-popover p-1 text-sm text-popover-foreground shadow-lg ring-1 ring-foreground/10 outline-none transition-[opacity,scale] duration-100 data-ending-style:scale-95 data-ending-style:opacity-0 data-starting-style:scale-95 data-starting-style:opacity-0",
            className,
          )}
          {...props}
        />
      </M.Positioner>
    </M.Portal>
  )
}

function MenuItem({ className, ...props }: MenuPrimitive.Item.Props) {
  const { Menu: M } = useLoadedPopups()
  return (
    <M.Item
      data-slot="menu-item"
      className={cn(
        "flex cursor-default items-center gap-2 rounded-lg px-2 py-1.5 outline-none select-none data-disabled:opacity-50 data-highlighted:bg-muted [&_svg]:size-4 [&_svg]:shrink-0 [&_svg]:text-muted-foreground",
        className,
      )}
      {...props}
    />
  )
}

export { Menu, MenuTrigger, MenuContent, MenuItem }
