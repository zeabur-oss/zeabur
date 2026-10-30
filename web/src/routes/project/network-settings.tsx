import { useQuery } from "@tanstack/react-query"
import { useState } from "react"

import type { Schemas } from "@/api/client"
import { projectQuery, projectsQuery } from "@/api/queries"
import { GateTip, useGates } from "@/components/gate"
import { usePlanner } from "@/components/plan/planner"
import { useT } from "@/i18n"
import type { Gate } from "@/model/rights"
import { allowFrom, networkPolicies, projectPolicy } from "@/model/isolation"
import { Button } from "@zeabur/design-system/components/button"
import { Card, CardRow, SectionLabel } from "@zeabur/design-system/components/card"
import { Select } from "@zeabur/design-system/components/select"

// A plan's `create` takes any object.
const asObject = (object: object) => object as Record<string, never>

type Planner = ReturnType<typeof usePlanner>
type Allowed = Schemas.NetworkPolicyAllowedNamespace

/** Whether other projects can reach this one, and the projects let in. */
export function NetworkSettings({ project }: { project: string }) {
  const t = useT()
  const reach = useQuery(projectQuery(project)).data?.extensions?.networkPolicy
  const planner = usePlanner(project)
  const gate = useGates(project)("changeNetworkPolicies")
  if (!reach) return null
  return (
    <section>
      <SectionLabel>{t("network.title")}</SectionLabel>
      <Card variant="list">
        <CardRow
          label={t("network.isolation")}
          description={reach.projectPolicy ? t("network.isolated") : t("network.open")}
        >
          {reach.projectPolicy ? null : (
            <GateTip gate={gate}>
              <Button
                variant="outline"
                size="sm"
                onClick={() =>
                  planner.plan({
                    title: t("fix.isolateProject", project),
                    changes: [{ op: "create", object: asObject(projectPolicy(project)) }],
                  })
                }
              >
                {t("network.isolate")}
              </Button>
            </GateTip>
          )}
        </CardRow>
        {reach.projectPolicy ? (
          <AllowedProjects
            project={project}
            allowed={reach.allowedNamespaces}
            gate={gate}
            planner={planner}
          />
        ) : null}
      </Card>
      {planner.dialog}
    </section>
  )
}

function AllowedProjects({
  project,
  allowed,
  gate,
  planner,
}: {
  project: string
  allowed: Allowed[]
  gate: Gate
  planner: Planner
}) {
  const t = useT()
  const projects = useQuery(projectsQuery).data?.items ?? []
  const choices = projects
    .filter((p) => p.adopted && !p.system && p.name !== project)
    .map((p) => p.name)
    .filter((name) => !allowed.some((a) => a.namespace === name))
  return (
    <>
      {allowed.map((a) => (
        <CardRow
          key={a.policy}
          label={<span translate="no">{a.namespace}</span>}
          description={t("network.allowedHint", a.policy)}
        >
          <GateTip gate={gate}>
            <Button
              variant="ghost"
              size="sm"
              onClick={() =>
                planner.plan({
                  title: t("network.disallowTitle", a.namespace, project),
                  changes: [{ op: "delete", resource: networkPolicies, name: a.policy }],
                })
              }
            >
              {t("network.disallow")}
            </Button>
          </GateTip>
        </CardRow>
      ))}
      <CardRow
        label={t("network.allow")}
        description={choices.length > 0 ? t("network.allowHint") : t("network.noOthers")}
        className="max-sm:items-stretch"
      >
        {choices.length > 0 ? (
          <AllowPicker project={project} choices={choices} gate={gate} planner={planner} />
        ) : null}
      </CardRow>
    </>
  )
}

function AllowPicker({
  project,
  choices,
  gate,
  planner,
}: {
  project: string
  choices: string[]
  gate: Gate
  planner: Planner
}) {
  const t = useT()
  const [chosen, setChosen] = useState("")
  const pick = choices.includes(chosen) ? chosen : (choices[0] ?? "")
  return (
    <span className="flex min-w-0 items-center gap-2">
      <Select
        size="sm"
        value={pick}
        onValueChange={setChosen}
        aria-label={t("network.allow")}
        translate="no"
        className="min-w-0 sm:w-48"
        items={choices.map((name) => ({ value: name, label: name }))}
      />
      <GateTip gate={gate}>
        <Button
          variant="outline"
          size="sm"
          onClick={() =>
            planner.plan({
              title: t("network.allowTitle", pick, project),
              changes: [{ op: "create", object: asObject(allowFrom(project, pick)) }],
            })
          }
        >
          {t("network.allowAction")}
        </Button>
      </GateTip>
    </span>
  )
}
