// The one place a plan, an object or a write offers its raw YAML: a switch between the plain
// description (the default) and the YAML, so a reader who wants the manifest knows where it is.
import { useState, type ReactNode } from "react"

import { Segmented } from "@/components/bits"
import { useT } from "@/i18n"

type View = "describe" | "yaml"

export function DescribeYaml({
  describe,
  yaml,
  className,
}: {
  describe: ReactNode
  /** Without it there is only the description and no switch. */
  yaml?: ReactNode
  className?: string
}) {
  const t = useT()
  const [view, setView] = useState<View>("describe")
  const frame = className ?? "flex flex-col gap-3"
  if (yaml === undefined || yaml === null) return <div className={frame}>{describe}</div>
  return (
    <div className={frame}>
      <Segmented
        label={t("describe.view")}
        value={view}
        onChange={setView}
        options={[
          { value: "describe", label: t("describe.describe") },
          { value: "yaml", label: t("tab.yaml") },
        ]}
      />
      {view === "describe" ? describe : yaml}
    </div>
  )
}
