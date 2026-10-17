import * as React from "react"
import { Field as FieldPrimitive } from "@base-ui/react/field"
import { cva, type VariantProps } from "class-variance-authority"
import { cn } from "cn"

const textareaVariants = cva(
  "flex field-sizing-content min-h-16 w-full rounded-lg border border-input bg-transparent transition-colors outline-none placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 disabled:cursor-not-allowed disabled:bg-input/50 disabled:opacity-50 aria-invalid:border-destructive aria-invalid:ring-3 aria-invalid:ring-destructive/20 dark:bg-input/30 dark:disabled:bg-input/80 dark:aria-invalid:border-destructive/50 dark:aria-invalid:ring-destructive/40",
  {
    variants: {
      size: {
        default: "px-2.5 py-2 text-base md:text-sm",
        sm: "min-h-7 resize-y rounded-md px-2 py-1 text-xs",
      },
      mono: { true: "type-mono", false: "" },
    },
    defaultVariants: { size: "default", mono: false },
  },
)

/** A textarea that is a Base UI Field control, so a FieldLabel names it and a FieldError
 * describes it. */
function Textarea({
  className,
  size,
  mono,
  ...props
}: React.ComponentProps<"textarea"> & VariantProps<typeof textareaVariants>) {
  return (
    <FieldPrimitive.Control
      render={<textarea />}
      data-slot="textarea"
      translate={mono ? "no" : undefined}
      className={cn(textareaVariants({ size, mono }), className)}
      // The control renders the textarea, which takes these as they are.
      {...(props as FieldPrimitive.Control.Props)}
    />
  )
}

export { Textarea }
