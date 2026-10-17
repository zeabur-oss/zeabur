import * as React from "react"
import { Input as InputPrimitive } from "@base-ui/react/input"
import { cva, type VariantProps } from "class-variance-authority"
import { cn } from "cn"

const inputVariants = cva(
  "w-full min-w-0 rounded-lg border border-input bg-transparent transition-colors outline-none file:inline-flex file:h-6 file:border-0 file:bg-transparent file:text-sm file:font-medium file:text-foreground placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 disabled:pointer-events-none disabled:cursor-not-allowed disabled:bg-input/50 disabled:opacity-50 aria-invalid:border-destructive aria-invalid:ring-3 aria-invalid:ring-destructive/20 dark:bg-input/30 dark:disabled:bg-input/80 dark:aria-invalid:border-destructive/50 dark:aria-invalid:ring-destructive/40",
  {
    variants: {
      size: {
        default: "h-8 px-2.5 py-1 text-base md:text-sm",
        // The forms' compact fields, where a row holds several.
        sm: "h-7 rounded-md px-2 py-0.5 text-xs",
      },
      // Machine values (names, quantities, hosts): Geist Mono, not translated.
      mono: { true: "type-mono", false: "" },
    },
    defaultVariants: { size: "default", mono: false },
  },
)

function Input({
  className,
  type,
  size,
  mono,
  ...props
}: Omit<React.ComponentProps<"input">, "size"> & VariantProps<typeof inputVariants>) {
  return (
    <InputPrimitive
      type={type}
      data-slot="input"
      translate={mono ? "no" : undefined}
      className={cn(inputVariants({ size, mono }), className)}
      {...props}
    />
  )
}

export { Input }
