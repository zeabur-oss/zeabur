"use client"

import { Field as FieldPrimitive } from "@base-ui/react/field"
import { cn } from "cn"

function Field({ className, ...props }: FieldPrimitive.Root.Props) {
  return (
    <FieldPrimitive.Root
      data-slot="field"
      className={cn("group/field flex min-w-0 flex-col gap-1", className)}
      {...props}
    />
  )
}

function FieldLabel({ className, ...props }: FieldPrimitive.Label.Props) {
  return (
    <FieldPrimitive.Label
      data-slot="field-label"
      className={cn("w-fit type-meta text-muted-foreground data-disabled:opacity-50", className)}
      {...props}
    />
  )
}

function FieldDescription({ className, ...props }: FieldPrimitive.Description.Props) {
  return (
    <FieldPrimitive.Description
      data-slot="field-description"
      className={cn("type-micro text-muted-foreground", className)}
      {...props}
    />
  )
}

function FieldError({ className, ...props }: FieldPrimitive.Error.Props) {
  return (
    <FieldPrimitive.Error
      data-slot="field-error"
      className={cn("type-micro text-fail", className)}
      {...props}
    />
  )
}

const FieldControl = FieldPrimitive.Control

export { Field, FieldControl, FieldDescription, FieldError, FieldLabel }
