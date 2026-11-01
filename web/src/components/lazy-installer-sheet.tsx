import { lazy, Suspense, useState, type ComponentProps, type ReactNode } from "react"

import { useT } from "@/i18n"
import type { Needed } from "@/model/installer"
import { Alert, AlertDescription, AlertTitle } from "@zeabur-oss/design-system/components/alert"
import { Button } from "@zeabur-oss/design-system/components/button"

const Sheet = lazy(() =>
  import("@/components/installer-sheet").then((m) => ({ default: m.InstallerSheet })),
)

/** The installer sheet, loaded when first opened. */
export function LazyInstallerSheet(props: ComponentProps<typeof Sheet>) {
  return (
    <Suspense fallback={null}>
      <Sheet {...props} />
    </Suspense>
  )
}

/** What the cluster lacks for something, with "How to install" opening the steps for all of it. */
export function InstallAlert({
  variant,
  icon,
  title,
  text,
  components,
  reason,
}: {
  variant: "info" | "warn"
  icon?: ReactNode
  title?: string
  text: string
  components: Needed[]
  reason?: string
}) {
  const t = useT()
  const [open, setOpen] = useState(false)
  return (
    <Alert variant={variant}>
      {icon}
      {title ? <AlertTitle>{title}</AlertTitle> : null}
      <AlertDescription>
        <p>{text}</p>
        <Button variant="outline" size="xs" onClick={() => setOpen(true)}>
          {t("installer.how")}
        </Button>
        {open ? (
          <LazyInstallerSheet
            components={components}
            reason={reason}
            onClose={() => setOpen(false)}
          />
        ) : null}
      </AlertDescription>
    </Alert>
  )
}
