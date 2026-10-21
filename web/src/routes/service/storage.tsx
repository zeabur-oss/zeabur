import { lazy, Suspense } from "react"
import FileTextIcon from "~icons/ph/file-text"
import HardDriveIcon from "~icons/ph/hard-drive"

import { Panel } from "@/components/bits"
import { VolumeBackups } from "@/components/backups"
import { KubeDetails } from "@/components/kube-details"
import { usePlanner } from "@/components/plan/planner"
import { StatusPill } from "@/components/status"
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@zeabur/design-system/components/empty"
import { SectionHeader } from "@zeabur/design-system/components/section-header"
import { useT, type Translate } from "@/i18n"
import { createdKey, entrance } from "@/lib/created"
import { claimOf, volumesOf, type Service } from "@/model/service"
import type { Schemas } from "@/api/client"

const ClaimForm = lazy(() =>
  import("@/routes/service/storage-forms").then((m) => ({ default: m.ClaimForm })),
)
const AddVolume = lazy(() =>
  import("@/routes/service/storage-forms").then((m) => ({ default: m.AddVolume })),
)

export function StorageTab({ project, service }: { project: string; service: Service }) {
  const t = useT()
  const planner = usePlanner(project)
  const volumes = volumesOf(service)
  const workload = { kind: service.kind, name: service.name }
  const container = service.containers.find((c) => !c.init)
  const managed = service.provenance.tool
  // A Job runs once; its template is changed as YAML.
  const editable = service.kind !== "Job"
  // A StatefulSet's volumes come from its claim templates, which cannot change.
  const addable = editable && service.kind !== "StatefulSet"
  const disabledReason = managed ? t("ops.managedEdit", managed) : undefined
  const other = service.containers.flatMap((c) =>
    c.mounts
      .filter((m) => m.kind !== "PersistentVolumeClaim")
      .map((m) => ({ container: c.name, mount: m })),
  )
  return (
    <div className="flex flex-col gap-4">
      <SectionHeader
        title={t("storage.title")}
        count={volumes.length > 0 ? volumes.length : undefined}
        description={t("storage.about")}
      />
      {volumes.length === 0 && addable ? null : volumes.length === 0 ? (
        <Empty variant="outline" size="inline">
          <EmptyMedia variant="glyph">
            <HardDriveIcon aria-hidden="true" />
          </EmptyMedia>
          <EmptyHeader>
            <EmptyTitle>{t("storage.emptyTitle", service.name)}</EmptyTitle>
            <EmptyDescription>
              {service.kind === "StatefulSet" ? t("form.statefulVolumes") : t("storage.emptyJob")}
            </EmptyDescription>
          </EmptyHeader>
        </Empty>
      ) : (
        <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
          {volumes.map((m) => {
            const name = claimOf(m)
            return (
              <Panel
                key={name}
                className={entrance(createdKey.volume(workload, m.mountPath))}
                title={
                  <span className="flex items-center gap-2">
                    <HardDriveIcon className="size-4 text-muted-foreground" aria-hidden="true" />
                    <span className="font-mono type-body leading-4.5" translate="no">
                      {m.mountPath}
                    </span>
                  </span>
                }
                action={
                  m.status ? (
                    <StatusPill health={m.status === "Bound" ? "ok" : "warn"} size="sm">
                      {phaseWord(m.status, t)}
                    </StatusPill>
                  ) : undefined
                }
              >
                <div className="flex flex-col gap-3">
                  <div className="flex items-baseline gap-2">
                    <span className="text-2xl font-semibold tabular-nums">{m.size ?? "—"}</span>
                    <span className="text-sm text-muted-foreground">{t("storage.requested")}</span>
                  </div>
                  {editable ? (
                    <Suspense fallback={null}>
                      <ClaimForm
                        mount={m}
                        disabledReason={disabledReason}
                        onSave={({ size, mount }) =>
                          planner.plan({
                            title:
                              size && mount
                                ? t("title.editVolume", name)
                                : size
                                  ? t("title.expand", name, size)
                                  : t("title.mount", m.mountPath),
                            changes: [
                              ...(size ? [{ op: "expandVolume" as const, claim: name, size }] : []),
                              ...(mount
                                ? [
                                    {
                                      op: "editMount" as const,
                                      workload,
                                      container: container?.name,
                                      mountPath: m.mountPath,
                                      newMountPath:
                                        mount.mountPath === m.mountPath
                                          ? undefined
                                          : mount.mountPath,
                                      readOnly:
                                        mount.readOnly === (m.readOnly ?? false)
                                          ? undefined
                                          : mount.readOnly,
                                    },
                                  ]
                                : []),
                            ],
                            service: workload,
                          })
                        }
                      />
                    </Suspense>
                  ) : null}
                </div>
              </Panel>
            )
          })}
        </div>
      )}
      {!editable ? null : !addable ? (
        volumes.length === 0 ? null : (
          <p className="type-meta text-muted-foreground">{t("form.statefulVolumes")}</p>
        )
      ) : (
        <Suspense fallback={null}>
          <AddVolume
            workload={service.name}
            title={
              volumes.length === 0 ? t("storage.emptyTitle", service.name) : t("form.addVolume")
            }
            description={volumes.length === 0 ? t("storage.emptyBody") : undefined}
            disabledReason={disabledReason}
            onPlan={(volume) =>
              planner.plan({
                title: t("title.addVolume", volume.mountPath),
                changes: [{ op: "addVolume", workload, container: container?.name, volume }],
              })
            }
          />
        </Suspense>
      )}
      {other.length > 0 ? (
        <Panel title={t("storage.otherMounts")}>
          <ul className="flex flex-col divide-y">
            {other.map(({ container, mount }) => (
              <li
                key={`${container}/${mount.mountPath}`}
                className="flex items-center gap-2 py-2 text-xs"
              >
                <FileTextIcon className="size-3.5 text-muted-foreground" aria-hidden="true" />
                <span className="font-mono" translate="no">
                  {mount.mountPath}
                </span>
                <span className="ml-auto text-muted-foreground">{mountWords(t, mount)}</span>
              </li>
            ))}
          </ul>
        </Panel>
      ) : null}
      <KubeDetails
        facts={[
          ...volumes.map((m): [string, string] => [
            m.mountPath,
            [
              `PersistentVolumeClaim/${claimOf(m)}`,
              m.status,
              m.accessModes?.join(","),
              m.storageClass ? `storageClassName: ${m.storageClass}` : undefined,
            ]
              .filter(Boolean)
              .join(" "),
          ]),
          ...other.map(({ mount }): [string, string] => [
            mount.mountPath,
            mount.object
              ? `${mount.object.kind}/${mount.object.name}`
              : `${mount.kind} ${mount.name}`,
          ]),
        ]}
      />
      <VolumeBackups project={project} claims={volumes.map(claimOf)} />
      {planner.dialog}
    </div>
  )
}

function phaseWord(phase: string, t: Translate): string {
  if (phase === "Bound") return t("storage.bound")
  if (phase === "Pending") return t("storage.pending")
  if (phase === "Lost") return t("storage.lost")
  return phase
}

function mountWords(t: Translate, mount: Schemas["Mount"]): string {
  const name = mount.object?.name ?? mount.name
  switch (mount.kind) {
    case "ConfigMap":
      return t("storage.fromConfig", name)
    case "Secret":
      return t("storage.fromSecret", name)
    case "EmptyDir":
    case "Ephemeral":
      return t("storage.scratch")
    case "DownwardAPI":
    case "Projected":
      return t("storage.fromCluster")
    default:
      return mount.name
  }
}
