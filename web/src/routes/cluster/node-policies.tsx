// The installer's baseline network policies let in the nodes they knew; a node that joins later
// is let in by a plan here, or by `zeabur-installer node sync` on a server.
import { useMutation, useQuery } from "@tanstack/react-query"
import { useState } from "react"
import CaretRightIcon from "~icons/ph/caret-right"

import { isStatus, type Schemas } from "@/api/client"
import { applyNodePolicies, nodePoliciesQuery, planNodePolicies, queryClient } from "@/api/queries"
import { Frame } from "@/components/members/frame"
import { useHintGate } from "@/components/gate"
import { LoadingRows } from "@/components/page"
import { useSecondsLeft } from "@/components/plan/expiry"
import { SealedFooter } from "@/components/plan/sealed-footer"
import { Problem } from "@/components/problem"
import { StatusDot } from "@/components/status"
import { YamlDiff } from "@/components/yaml"
import { useT } from "@/i18n"
import { policiesState, SYNC_COMMAND } from "@/model/join"
import { Button } from "@zeabur/design-system/components/button"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@zeabur/design-system/components/collapsible"

const refresh = () => {
  void queryClient.invalidateQueries({ queryKey: nodePoliciesQuery.queryKey })
}

/** A sentence with the sync command in it, set as code. */
function WithCommand({ text }: { text: (command: string) => string }) {
  const [before, after] = text(SYNC_COMMAND).split(SYNC_COMMAND)
  return (
    <span>
      {before}
      <code className="type-mono text-foreground" translate="no">
        {SYNC_COMMAND}
      </code>
      {after}
    </span>
  )
}

/** What the person can do about policies that are behind: review the plan, or run the command on
 * a server when they may not change the policies. */
function Update({ policies }: { policies: Schemas["NodePolicies"] }) {
  const t = useT()
  const gate = useHintGate("changeNetworkPolicies", policies.canUpdate)
  const [open, setOpen] = useState(false)
  if (!gate.open)
    return (
      <span className="type-meta text-muted-foreground">
        <WithCommand text={(c) => t("policies.cannotUpdate", c)} />
      </span>
    )
  return (
    <>
      <Button variant="outline" size="xs" className="sm:ml-auto" onClick={() => setOpen(true)}>
        {t("edit.review")}
      </Button>
      {open ? <PoliciesPlan onClose={() => setOpen(false)} /> : null}
    </>
  )
}

/** The step after a node is Ready: let its traffic through the network policies. Nothing on a
 * cluster without them. */
export function PoliciesStep() {
  const t = useT()
  const policies = useQuery(nodePoliciesQuery)
  if (!policies.data) return null
  const state = policiesState(policies.data)
  if (state === "none") return null
  return (
    <li>
      <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
        {state === "unreadable" ? (
          <span className="text-muted-foreground">
            <WithCommand text={(c) => t("policies.unreadable", c)} />
          </span>
        ) : state === "current" ? (
          <span className="flex items-center gap-2">
            <StatusDot health="ok" />
            {t("policies.current")}
          </span>
        ) : (
          <>
            <span>{state === "behind" ? t("policies.step") : t("policies.stale")}</span>
            <Update policies={policies.data} />
          </>
        )}
      </div>
    </li>
  )
}

/** Cluster → Nodes: a line while a node's traffic is dropped by the policies, or a removed node's
 * is still let in. */
export function PoliciesBehind() {
  const t = useT()
  const policies = useQuery(nodePoliciesQuery)
  const state = policies.data && policiesState(policies.data)
  if (!policies.data || (state !== "behind" && state !== "stale")) return null
  return (
    <div className="flex flex-wrap items-center gap-x-3 gap-y-2 type-meta">
      <span className="flex items-center gap-2">
        <StatusDot health="warn" />
        {state === "behind"
          ? t(
              "policies.behind",
              policies.data.behind.map((node) => node.name),
            )
          : t("policies.stale")}
      </span>
      <Update policies={policies.data} />
    </div>
  )
}

function PoliciesPlan({ onClose }: { onClose: () => void }) {
  const t = useT()
  // Planning writes nothing, so it is a query.
  const plan = useQuery({
    queryKey: ["node-policies-plan"],
    queryFn: ({ signal }) => planNodePolicies(signal),
    retry: false,
    staleTime: Infinity,
    gcTime: 0,
  })
  const apply = useMutation({
    mutationFn: (token: string) => applyNodePolicies(token),
    onSuccess: () => {
      refresh()
      onClose()
    },
    onError: refresh,
  })
  const left = useSecondsLeft(plan.data?.expiresAt)
  const writes = plan.data?.writes ?? []
  const nothing = plan.data !== undefined && writes.length === 0
  const ready = plan.data && !plan.isFetching
  return (
    <Frame
      title={t("policies.planTitle")}
      wide
      onClose={onClose}
      footer={
        <SealedFooter
          left={left}
          ready={Boolean(ready)}
          back={
            <Button variant="ghost" size="sm" onClick={onClose}>
              {t("plan.cancel")}
            </Button>
          }
          onReplan={() => void plan.refetch()}
        >
          <Button
            size="sm"
            disabled={!ready || nothing}
            loading={apply.isPending}
            onClick={() => plan.data && apply.mutate(plan.data.token)}
          >
            {t("plan.apply")}
          </Button>
        </SealedFooter>
      }
    >
      {plan.isPending || plan.isFetching ? (
        <LoadingRows label={t("plan.planning")} rows={3} />
      ) : plan.isError ? (
        <Refused error={plan.error} />
      ) : nothing ? (
        <p>{t("policies.current")}</p>
      ) : (
        <>
          <p>
            {t(
              "policies.planLine",
              writes.map((w) => w.namespace),
            )}
          </p>
          <Writes writes={writes} />
        </>
      )}
      {apply.isError ? <Refused error={apply.error} /> : null}
    </Frame>
  )
}

/** The problem, and for a refusal, the command that does the same on a server. */
function Refused({ error }: { error: unknown }) {
  const t = useT()
  return (
    <>
      <Problem error={error} />
      {isStatus(error, 403) ? (
        <p className="type-meta text-muted-foreground">
          <WithCommand text={(c) => t("policies.cannotUpdate", c)} />
        </p>
      ) : null}
    </>
  )
}

function Writes({ writes }: { writes: Schemas["PolicyWrite"][] }) {
  const t = useT()
  return (
    <Collapsible>
      <CollapsibleTrigger variant="disclosure" className="mt-1">
        <CaretRightIcon aria-hidden="true" />
        {t("plan.details")}
      </CollapsibleTrigger>
      <CollapsibleContent>
        <div className="flex flex-col gap-4 pt-3">
          {writes.map((write) => (
            <section
              key={write.namespace}
              aria-label={`${write.namespace}/${write.name}`}
              className="flex min-w-0 flex-col gap-2"
            >
              <span className="min-w-0 font-mono text-sm font-medium break-all" translate="no">
                {`NetworkPolicy/${write.name}`}
                <span className="ml-2 font-sans font-normal type-meta text-muted-foreground">
                  {write.namespace}
                </span>
              </span>
              <YamlDiff before={write.beforeYaml ?? ""} after={write.afterYaml ?? ""} />
            </section>
          ))}
        </div>
      </CollapsibleContent>
    </Collapsible>
  )
}
