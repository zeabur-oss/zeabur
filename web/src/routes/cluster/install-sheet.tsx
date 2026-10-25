import { keepPreviousData as keep, useQuery } from "@tanstack/react-query"
import { useState } from "react"
import WarningFillIcon from "~icons/ph/warning-fill"
import XIcon from "~icons/ph/x"

import type { Schemas } from "@/api/client"
import { installQuery } from "@/api/queries"
import { Segmented } from "@/components/bits"
import { GateTip, useClusterGate } from "@/components/gate"
import { LoadingRows } from "@/components/page"
import { usePlanner, type PlanAsk } from "@/components/plan/planner"
import { Problem } from "@/components/problem"
import { Alert, AlertDescription } from "@zeabur/design-system/components/alert"
import { Badge } from "@zeabur/design-system/components/badge"
import { Button } from "@zeabur/design-system/components/button"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogTitle,
} from "@zeabur/design-system/components/dialog"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@zeabur/design-system/components/tabs"
import { YamlBlock } from "@/components/yaml"
import { useT } from "@/i18n"
import { k3sPackagedNotes } from "@/routes/cluster/k3s-packaged"
import type { profiles } from "@/routes/search"

type Profile = (typeof profiles)[number]
type Format = Schemas["InstallFormat"]

const formatNames = {
  helm: "Helm",
  k3s: "k3s HelmChart",
  flux: "Flux",
  argoCd: "Argo CD",
} as const

export function InstallSheet({
  component,
  name,
  notes,
  installed,
  profile,
  onProfile,
  onClose,
}: {
  /** `capability/id`. */
  component: string
  name: string
  /** The catalogue's description: licence, sizing and packaging, in English. */
  notes?: string
  /** Detection found this component in the cluster. */
  installed: boolean
  profile?: Profile
  onProfile: (profile: Profile) => void
  onClose: () => void
}) {
  const t = useT()
  const [capability = "", id = ""] = component.split("/")
  const guide = useQuery({ ...installQuery(capability, id, profile), placeholderData: keep })
  const data = guide.data
  const [chosen, setFormat] = useState<Format>()
  const format =
    data?.formats.find((f) => f.format === chosen) ??
    data?.formats.find((f) => f.objects.length > 0) ??
    data?.formats[0]
  const planner = usePlanner(format?.namespace ?? "kube-system", onClose, true)
  return (
    <Dialog open onOpenChange={(open) => (open ? undefined : onClose())}>
      <DialogContent variant="panel">
        <div className="flex items-center gap-2 border-b px-6 py-3">
          <DialogTitle className="flex min-w-0 flex-1 items-baseline">
            <span className="truncate">{name}</span>
            <span className="ml-2 shrink-0 type-meta font-normal text-muted-foreground">
              {t("install.title")}
            </span>
          </DialogTitle>
          <DialogClose
            render={
              <Button
                variant="ghost"
                size="icon-sm"
                className="-mr-1"
                aria-label={t("common.close")}
              />
            }
          >
            <XIcon aria-hidden="true" />
          </DialogClose>
        </div>
        <div className="flex min-h-0 flex-1 flex-col gap-5 overflow-y-auto px-6 py-5">
          {guide.isPending ? (
            <LoadingRows label={t("common.loading")} rows={8} />
          ) : guide.isError ? (
            <Problem error={guide.error} />
          ) : data ? (
            <>
              {k3sPackagedNotes(data.replacesK3sPackaged, t).map((note) => (
                <Alert key={note} variant="warn" role="note">
                  <WarningFillIcon aria-hidden="true" />
                  <AlertDescription>{note}</AlertDescription>
                </Alert>
              ))}
              <div className="flex flex-wrap items-center gap-2">
                <span className="type-meta text-muted-foreground">{t("install.profile")}</span>
                <Segmented
                  label={t("install.profile")}
                  value={data.profile as Profile}
                  onChange={onProfile}
                  options={(["single-node", "multi-node"] as const).map((p) => ({
                    value: p,
                    label: (
                      <>
                        {p === "single-node" ? t("install.singleNode") : t("install.multiNode")}
                        {p === data.defaultProfile ? (
                          <span className="opacity-60">{t("install.forCluster")}</span>
                        ) : null}
                      </>
                    ),
                  }))}
                />
              </div>
              <dl className="grid grid-cols-1 gap-x-4 gap-y-2 type-meta sm:grid-cols-pairs">
                {notes ? (
                  <>
                    <dt className="text-muted-foreground">{t("install.notes")}</dt>
                    <dd lang="en">{notes}</dd>
                  </>
                ) : null}
                {data.requires.length > 0 ? (
                  <>
                    <dt className="text-muted-foreground">{t("install.needs")}</dt>
                    <dd className="flex flex-wrap gap-1.5">
                      {data.requires.map((d) => (
                        <Badge key={d} variant="outline" translate="no">
                          {d}
                        </Badge>
                      ))}
                    </dd>
                  </>
                ) : null}
                <dt className="text-muted-foreground">{t("install.security")}</dt>
                <dd className="flex flex-col gap-1">
                  <span>{data.security.summary}</span>
                  <span className="flex flex-wrap gap-1.5">
                    {data.security.clusterWide ? (
                      <Badge variant="warn">{t("install.clusterWide")}</Badge>
                    ) : null}
                    {data.security.privileged ? (
                      <Badge variant="fail">{t("install.privileged")}</Badge>
                    ) : null}
                  </span>
                </dd>
                {(data.security.egress ?? []).length > 0 ? (
                  <>
                    <dt className="text-muted-foreground">{t("install.connectsTo")}</dt>
                    <dd className="type-mono" translate="no">
                      {(data.security.egress ?? []).join(", ")}
                    </dd>
                  </>
                ) : null}
                {data.security.credentials ? (
                  <>
                    <dt className="text-muted-foreground">{t("install.credentials")}</dt>
                    <dd>{data.security.credentialsNote ?? data.security.credentials}</dd>
                  </>
                ) : null}
                {(data.security.risks ?? []).length > 0 ? (
                  <>
                    <dt className="text-muted-foreground">{t("install.risks")}</dt>
                    <dd>
                      <ul className="flex flex-col gap-1">
                        {(data.security.risks ?? []).map((r) => (
                          <li key={r.risk}>
                            {r.risk} <span className="text-muted-foreground">{r.mitigation}</span>
                          </li>
                        ))}
                      </ul>
                    </dd>
                  </>
                ) : null}
                {data.inputs.length > 0 ? (
                  <>
                    <dt className="text-muted-foreground">{t("install.inputs")}</dt>
                    <dd>
                      <ul className="flex flex-col gap-1">
                        {data.inputs.map((input) => (
                          <li key={input.name}>
                            <code className="type-mono" translate="no">
                              &lt;{input.name}&gt;
                            </code>{" "}
                            <span className="text-muted-foreground">
                              {input.description}
                              {input.generated
                                ? `${t("common.sep")}${t("install.generated")}`
                                : input.required
                                  ? `${t("common.sep")}${t("install.required")}`
                                  : ""}
                            </span>
                          </li>
                        ))}
                      </ul>
                    </dd>
                  </>
                ) : null}
                {data.contributesTo.length > 0 ? (
                  <>
                    <dt className="text-muted-foreground">{t("install.alsoAdds")}</dt>
                    <dd className="type-mono" translate="no">
                      {data.contributesTo.join(", ")}
                    </dd>
                  </>
                ) : null}
              </dl>
              <section className="flex flex-col gap-3 border-t pt-5">
                <Tabs value={format?.format} onValueChange={(value) => setFormat(value as Format)}>
                  <TabsList>
                    {data.formats.map((f) => (
                      <TabsTrigger key={f.format} value={f.format}>
                        {formatNames[f.format]}
                      </TabsTrigger>
                    ))}
                  </TabsList>
                  {data.formats.map((f) => (
                    <TabsContent key={f.format} value={f.format}>
                      <div className="flex flex-col gap-3">
                        <FormatNote
                          text={f}
                          name={name}
                          blocked={data.blocked}
                          installed={installed}
                          alone={data.formats.length === 1}
                          onPlan={planner.plan}
                        />
                        <YamlBlock yaml={f.text} label={`${name} ${formatNames[f.format]}`} />
                      </div>
                    </TabsContent>
                  ))}
                </Tabs>
              </section>
            </>
          ) : null}
        </div>
        {planner.dialog}
      </DialogContent>
    </Dialog>
  )
}

const tools = {
  k3s: "install.tool.k3s",
  flux: "install.tool.flux",
  argoCd: "install.tool.argoCd",
} as const

/** What a form's text is for: a plan Zeabur can run, or why the person runs it. */
function FormatNote({
  text,
  name,
  blocked,
  installed,
  alone,
  onPlan,
}: {
  text: Schemas["InstallText"]
  name: string
  blocked?: Schemas["InstallBlocked"] | null
  installed: boolean
  /** No controller for any object form is served. */
  alone: boolean
  onPlan: (ask: PlanAsk) => void
}) {
  const t = useT()
  const gate = useClusterGate("installComponents")
  if (text.format === "helm")
    return (
      <p className="type-meta text-pretty text-muted-foreground">
        {t("install.helm")}
        {alone && !blocked ? ` ${t("install.noController")}` : ""}
      </p>
    )
  if (installed) return <p className="type-meta text-muted-foreground">{t("install.installed")}</p>
  if (blocked || text.objects.length === 0)
    return (
      <p className="type-meta text-pretty text-muted-foreground">
        {blocked ? `${t(`install.blocked.${blocked}`)} ` : ""}
        {t("install.yourself")}
      </p>
    )
  const tool = t(tools[text.format])
  const plan = () =>
    onPlan({
      title: t("install.planTitle", name),
      changes: text.objects.map((yaml) => ({ op: "create", yaml })),
      notes: [text.format === "argoCd" ? t("install.after", tool) : t("install.afterRemove", tool)],
      view: "/cluster/components",
    })
  return (
    <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
      <p className="min-w-0 flex-1 text-pretty">
        {t("install.planWhat", text.namespace ?? "", tool)}
      </p>
      <GateTip gate={gate}>
        <Button size="sm" onClick={plan}>
          {t("install.planAction")}
        </Button>
      </GateTip>
    </div>
  )
}
