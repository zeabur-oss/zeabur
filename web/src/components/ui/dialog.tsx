import type { Dialog as DialogPrimitive } from "@base-ui/react/dialog"
import { cva, type VariantProps } from "class-variance-authority"
import { cn } from "cn"

import {
  PendingRoot,
  useLoadedPopups,
  useOpen,
  usePopups,
  type RootProps,
} from "@/components/ui/popup"

// Base UI's dialog, whose code loads after the first page (popup.tsx); one opened before it is
// there shows when it arrives.
function Dialog({
  open,
  defaultOpen,
  onOpenChange,
  ...props
}: RootProps<DialogPrimitive.Root.Props>) {
  const state = useOpen(open, defaultOpen, onOpenChange)
  if (!state.parts) return <PendingRoot setOpen={state.setOpen}>{props.children}</PendingRoot>
  return (
    <state.parts.Dialog.Root
      data-slot="dialog"
      {...props}
      open={state.open}
      onOpenChange={(next) => state.setOpen(next)}
    />
  )
}

/** `sheet`: a panel along the left edge, full height (the navigation on phones), sliding in from
 * that edge. `panel`: along the right edge, full height (an object's YAML, an editor), coming in a
 * little from the right. Neither moves under reduced motion. `wide`: a centred dialog for long
 * content (a plan's diff). `compact`: the default width, scrolling inside (a light confirmation). */
const dialogContentVariants = cva(
  "fixed top-[12vh] left-1/2 z-50 w-[calc(100%-2rem)] max-w-xl -translate-x-1/2 overflow-hidden rounded-2xl bg-popover text-popover-foreground shadow-2xl ring-1 ring-foreground/10 outline-none transition-[opacity,scale] duration-150 data-ending-style:scale-[0.98] data-ending-style:opacity-0 data-starting-style:scale-[0.98] data-starting-style:opacity-0",
  {
    variants: {
      variant: {
        default: "",
        sheet:
          "top-0 left-0 h-svh w-[min(20rem,88vw)] max-w-none translate-x-0 rounded-none rounded-r-2xl transition-[opacity,translate] duration-200 ease-out data-ending-style:scale-100 data-starting-style:scale-100 motion-safe:data-ending-style:-translate-x-full motion-safe:data-starting-style:-translate-x-full",
        panel:
          "top-0 right-0 left-auto flex h-svh w-[min(44rem,100vw)] max-w-none translate-x-0 flex-col rounded-none rounded-l-2xl transition-[opacity,translate] duration-200 ease-out data-ending-style:scale-100 data-starting-style:scale-100 motion-safe:data-ending-style:translate-x-8 motion-safe:data-starting-style:translate-x-8",
        wide: "top-[6vh] flex max-h-[88vh] max-w-3xl flex-col",
        compact: "flex max-h-[88vh] flex-col",
      },
    },
    defaultVariants: {
      variant: "default",
    },
  },
)

function DialogContent({
  className,
  backdropClassName,
  variant = "default",
  ...props
}: DialogPrimitive.Popup.Props &
  VariantProps<typeof dialogContentVariants> & { backdropClassName?: string }) {
  const parts = usePopups()
  // Closed until the code is there.
  if (!parts) return null
  const { Dialog: D } = parts
  return (
    <D.Portal>
      <D.Backdrop
        className={cn(
          "fixed inset-0 z-50 bg-black/30 transition-opacity duration-150 data-ending-style:opacity-0 data-starting-style:opacity-0 dark:bg-black/50",
          backdropClassName,
        )}
      />
      <D.Popup
        data-slot="dialog-content"
        className={cn(dialogContentVariants({ variant, className }))}
        {...props}
      />
    </D.Portal>
  )
}

function DialogTitle({ className, ...props }: DialogPrimitive.Title.Props) {
  const { Dialog: D } = useLoadedPopups()
  return (
    <D.Title
      data-slot="dialog-title"
      className={cn("text-base font-medium", className)}
      {...props}
    />
  )
}

function DialogClose(props: DialogPrimitive.Close.Props) {
  const { Dialog: D } = useLoadedPopups()
  return <D.Close {...props} />
}

export { Dialog, DialogContent, DialogTitle, DialogClose }
