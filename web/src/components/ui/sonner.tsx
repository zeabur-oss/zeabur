import { Toaster as Sonner, type ToasterProps } from "sonner"
import CheckCircleFillIcon from "~icons/ph/check-circle-fill"
import InfoIcon from "~icons/ph/info"
import SpinnerIcon from "~icons/ph/spinner"
import WarningFillIcon from "~icons/ph/warning-fill"
import XCircleFillIcon from "~icons/ph/x-circle-fill"

import { useTheme } from "@/theme"

// shadcn/ui's Sonner, with engawa's theme store in place of next-themes. Sonner adds its own styles
// with a <style> element when this chunk loads, which the page's Content Security Policy allows
// (`style-src 'unsafe-inline'`; web/DESIGN.md, "Content Security Policy"). The custom properties
// below override its colours and radius with the theme's.
const Toaster = ({ ...props }: ToasterProps) => {
  const theme = useTheme()

  return (
    <Sonner
      theme={theme}
      className="toaster group"
      icons={{
        success: <CheckCircleFillIcon className="size-4 text-ok" aria-hidden="true" />,
        info: <InfoIcon className="size-4" aria-hidden="true" />,
        warning: <WarningFillIcon className="size-4 text-warn" aria-hidden="true" />,
        error: <XCircleFillIcon className="size-4 text-fail" aria-hidden="true" />,
        loading: <SpinnerIcon className="size-4 animate-spin" aria-hidden="true" />,
      }}
      style={
        {
          "--normal-bg": "var(--popover)",
          "--normal-text": "var(--popover-foreground)",
          "--normal-border": "var(--border)",
          "--border-radius": "var(--radius)",
        } as React.CSSProperties
      }
      {...props}
    />
  )
}

export { Toaster }
