import type { Popover as PopoverPrimitive } from "@base-ui/react/popover"
import { cva, type VariantProps } from "class-variance-authority"
import { cn } from "cn"

import {
  PendingRoot,
  PendingTrigger,
  useOpen,
  usePopups,
  type RootProps,
  type TriggerProps,
} from "@/components/ui/popup"

// Base UI's popover, whose code loads after the first page (popup.tsx).
function Popover({
  open,
  defaultOpen,
  onOpenChange,
  ...props
}: RootProps<PopoverPrimitive.Root.Props>) {
  const state = useOpen(open, defaultOpen, onOpenChange)
  if (!state.parts) return <PendingRoot setOpen={state.setOpen}>{props.children}</PendingRoot>
  return (
    <state.parts.Popover.Root
      data-slot="popover"
      {...props}
      open={state.open}
      onOpenChange={(next) => state.setOpen(next)}
    />
  )
}

const popoverTriggerVariants = cva("", {
  variants: {
    variant: {
      default: "",
      ghost:
        "rounded-md text-muted-foreground outline-none hover:bg-muted hover:text-foreground focus-visible:ring-3 focus-visible:ring-ring aria-expanded:bg-muted aria-expanded:text-foreground",
      plain:
        "rounded-lg text-muted-foreground outline-none hover:text-foreground focus-visible:ring-3 focus-visible:ring-ring",
    },
  },
  defaultVariants: {
    variant: "default",
  },
})

function PopoverTrigger({
  className,
  variant = "default",
  ...props
}: TriggerProps<PopoverPrimitive.Trigger.Props> & VariantProps<typeof popoverTriggerVariants>) {
  const parts = usePopups()
  const classes = cn(popoverTriggerVariants({ variant, className }))
  if (!parts)
    return (
      <PendingTrigger
        haspopup="dialog"
        data-slot="popover-trigger"
        className={classes}
        {...props}
      />
    )
  return <parts.Popover.Trigger data-slot="popover-trigger" className={classes} {...props} />
}

function PopoverContent({
  className,
  align = "start",
  side = "bottom",
  sideOffset = 6,
  ...props
}: PopoverPrimitive.Popup.Props &
  Pick<PopoverPrimitive.Positioner.Props, "align" | "side" | "sideOffset">) {
  const parts = usePopups()
  // Closed until the code is there.
  if (!parts) return null
  const { Popover: P } = parts
  return (
    <P.Portal>
      <P.Positioner className="isolate z-50" align={align} side={side} sideOffset={sideOffset}>
        <P.Popup
          data-slot="popover-content"
          className={cn(
            "z-50 w-72 origin-(--transform-origin) rounded-xl bg-popover p-1 text-sm text-popover-foreground shadow-lg ring-1 ring-foreground/10 outline-none transition-[opacity,scale] duration-100 data-ending-style:scale-95 data-ending-style:opacity-0 data-starting-style:scale-95 data-starting-style:opacity-0",
            className,
          )}
          {...props}
        />
      </P.Positioner>
    </P.Portal>
  )
}

export { Popover, PopoverTrigger, PopoverContent, popoverTriggerVariants }
