import { Tooltip as TooltipPrimitive } from "@base-ui/react/tooltip"
import { cn } from "cn"
import { useState, type ReactElement, type ReactNode } from "react"

import { PendingTooltipTrigger, usePopups } from "@/components/ui/popup"

/** Tooltips next to each other share their delay: once one shows, the next shows at once. Only
 * the provider loads with the first page; tooltips themselves load later (popup.tsx). */
function TooltipProvider(props: TooltipPrimitive.Provider.Props) {
  return <TooltipPrimitive.Provider delay={250} {...props} />
}

/** A short text on hover and focus. `children` must be a single element that takes a ref. */
function Tooltip({
  content,
  children,
  side = "top",
  className,
}: {
  content: ReactNode
  children: ReactElement
  side?: TooltipPrimitive.Positioner.Props["side"]
  className?: string
}) {
  const parts = usePopups()
  // Pointed at or focused before the code arrived: shown when it does.
  const [shown, setShown] = useState(false)
  if (!parts) return <PendingTooltipTrigger onShow={setShown}>{children}</PendingTooltipTrigger>
  const { Tooltip: T } = parts
  return (
    <T.Root defaultOpen={shown}>
      <T.Trigger render={children} />
      <T.Portal>
        <T.Positioner side={side} sideOffset={6} className="isolate z-[60]">
          <T.Popup
            className={cn(
              "max-w-xs origin-(--transform-origin) rounded-md bg-foreground px-2 py-1 text-xs text-background shadow-md transition-[opacity,scale] duration-100 data-ending-style:opacity-0 data-starting-style:scale-95 data-starting-style:opacity-0",
              className,
            )}
          >
            {content}
          </T.Popup>
        </T.Positioner>
      </T.Portal>
    </T.Root>
  )
}

export { Tooltip, TooltipProvider }
