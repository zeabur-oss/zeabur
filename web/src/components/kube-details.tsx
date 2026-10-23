// The one place a section names its Kubernetes objects and fields, folded away (docs/wording.md,
// "Kubernetes terms").
import type { ReactElement, ReactNode } from "react"
import CaretRightIcon from "~icons/ph/caret-right"

import { useT } from "@/i18n"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@zeabur/design-system/components/collapsible"
import { Tooltip } from "@zeabur/design-system/components/tooltip"

type Facts = [ReactNode, ReactNode][]

function FactList({ facts }: { facts: Facts }) {
  return (
    <dl className="grid grid-cols-pairs gap-x-4 gap-y-1.5 pt-2 text-xs">
      {facts.map(([label, value], i) => (
        <div key={i} className="contents">
          <dt className="text-secondary">{label}</dt>
          <dd className="min-w-0 font-mono break-all" translate="no">
            {value}
          </dd>
        </div>
      ))}
    </dl>
  )
}

/** Pairs of a plain word and its Kubernetes object or field. */
export function KubeDetails({ facts }: { facts: Facts }) {
  const t = useT()
  if (facts.length === 0) return null
  return (
    <Collapsible>
      <CollapsibleTrigger variant="disclosure">
        <CaretRightIcon aria-hidden="true" />
        {t("kube.details")}
      </CollapsibleTrigger>
      <CollapsibleContent>
        <FactList facts={facts} />
      </CollapsibleContent>
    </Collapsible>
  )
}

/** The same pairs on hover, for a label with no room below it (a map's lane, a schedule). */
export function KubeTip({ facts, children }: { facts: Facts; children: ReactElement }) {
  const t = useT()
  if (facts.length === 0) return children
  return (
    <Tooltip
      content={
        <>
          <span className="font-medium">{t("kube.details")}</span>
          <FactList facts={facts} />
        </>
      }
    >
      {children}
    </Tooltip>
  )
}
