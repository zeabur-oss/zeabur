// How to install a catalogue component yourself, in a panel along the right edge: what it needs
// first, what access it gets, the values to fill in, and the texts in four forms (Helm, k3s,
// Flux, Argo CD). A component k3s also packages carries a warning to disable k3s's copy first. The server renders them from the catalogue; Engawa runs none of them.
import { keepPreviousData as keep, useQuery } from "@tanstack/react-query"
import { useState } from "react"
import WarningFillIcon from "~icons/ph/warning-fill"
import XIcon from "~icons/ph/x"

import { installQuery } from "@/api/queries"
import { Segmented } from "@/components/bits"
import { LoadingRows } from "@/components/page"
import { Problem } from "@/components/problem"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Dialog, DialogClose, DialogContent, DialogTitle } from "@/components/ui/dialog"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { YamlBlock } from "@/components/yaml"
import { useT } from "@/i18n"
import { k3sPackagedNotes } from "@/routes/cluster/k3s-packaged"
import type { profiles } from "@/routes/search"

type Profile = (typeof profiles)[number]

const formatNames = {
  helm: "Helm",
  k3s: "k3s HelmChart",
  flux: "Flux",
  argoCd: "Argo CD",
} as const

export function InstallSheet({
  component,
  name,
  profile,
  onProfile,
  onClose,
}: {
  /** `capability/id`. */
  component: string
  name: string
  profile?: Profile
  onProfile: (profile: Profile) => void
  onClose: () => void
}) {
  const t = useT()
  const [capability = "", id = ""] = component.split("/")
  // The previous profile's texts stay while the next load, and the chosen form stays chosen.
  const guide = useQuery({ ...installQuery(capability, id, profile), placeholderData: keep })
  const [format, setFormat] = useState<string>("helm")
  const data = guide.data
  return (
    <Dialog open onOpenChange={(open) => (open ? undefined : onClose())}>
      <DialogContent variant="panel">
        <div className="flex items-center gap-2 border-b px-5 py-3">
          <DialogTitle className="flex min-w-0 flex-1 items-baseline">
            <span className="truncate">{name}</span>
            <span className="ml-2 shrink-0 type-meta font-normal text-muted-foreground">
              {t("install.title")}
            </span>
          </DialogTitle>
          <DialogClose
            render={<Button variant="ghost" size="icon-sm" aria-label={t("common.close")} />}
          >
            <XIcon aria-hidden="true" />
          </DialogClose>
        </div>
        <div className="flex min-h-0 flex-1 flex-col gap-5 overflow-y-auto p-5">
          {guide.isPending ? (
            <LoadingRows label={t("common.loading")} rows={8} />
          ) : guide.isError ? (
            <Problem error={guide.error} />
          ) : data ? (
            <>
              <p className="type-meta text-muted-foreground">{t("install.note")}</p>
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
                {data.dependsOn.length > 0 ? (
                  <>
                    <dt className="text-muted-foreground">{t("install.needs")}</dt>
                    <dd className="flex flex-wrap gap-1.5">
                      {data.dependsOn.map((d) => (
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
              <Tabs value={format} onValueChange={(value) => setFormat(String(value))}>
                <TabsList>
                  {data.formats.map((f) => (
                    <TabsTrigger key={f.format} value={f.format}>
                      {formatNames[f.format]}
                    </TabsTrigger>
                  ))}
                </TabsList>
                {data.formats.map((f) => (
                  <TabsContent key={f.format} value={f.format}>
                    <YamlBlock yaml={f.text} label={`${name} ${formatNames[f.format]}`} />
                  </TabsContent>
                ))}
              </Tabs>
            </>
          ) : null}
        </div>
      </DialogContent>
    </Dialog>
  )
}
