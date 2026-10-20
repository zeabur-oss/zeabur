// A service's storage: one card per PersistentVolumeClaim (requested size, status, access mode,
// storage class, mount path) with its forms to grow the claim and move or protect the mount, then
// adding a volume, the other mounts and backups. The forms load with the form library, after the
// cards; every one ends in the plan dialog.
import { lazy, Suspense } from "react"
import FileTextIcon from "~icons/ph/file-text"
import HardDriveIcon from "~icons/ph/hard-drive"

import { Panel } from "@/components/bits"
import { VolumeBackups } from "@/components/backups"
import { usePlanner } from "@/components/plan/planner"
import { StatusPill } from "@/components/status"
import { Tooltip } from "@zeabur/design-system/components/tooltip"
import { useT } from "@/i18n"
import { claimOf, volumesOf, type Service } from "@/model/service"

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
  const other = service.containers.flatMap((c) =>
    c.mounts
      .filter((m) => m.kind !== "PersistentVolumeClaim")
      .map((m) => ({ container: c.name, mount: m })),
  )
  return (
    <div className="flex flex-col gap-4">
      {managed && editable ? (
        <p className="rounded-lg border border-dashed px-3 py-2 text-sm text-muted-foreground">
          {t("common.managedByDetail", managed)}
        </p>
      ) : null}
      {volumes.length === 0 ? (
        <div className="flex items-center gap-2 rounded-xl border border-dashed px-4 py-3 text-sm text-muted-foreground">
          <HardDriveIcon className="size-4" aria-hidden="true" />
          {t("storage.noClaims")}
        </div>
      ) : (
        <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
          {volumes.map((m) => {
            const name = claimOf(m)
            return (
              <Panel
                key={name}
                title={
                  <span className="flex items-center gap-2">
                    <HardDriveIcon className="size-4 text-muted-foreground" aria-hidden="true" />
                    <Tooltip content="PersistentVolumeClaim">
                      <span className="font-mono type-body leading-4.5" translate="no">
                        {name}
                      </span>
                    </Tooltip>
                  </span>
                }
                action={
                  m.status ? (
                    <StatusPill health={m.status === "Bound" ? "ok" : "warn"} size="sm">
                      {m.status}
                    </StatusPill>
                  ) : undefined
                }
              >
                <div className="flex flex-col gap-3">
                  <div className="flex items-baseline gap-2">
                    <span className="text-2xl font-semibold tabular-nums">{m.size ?? "—"}</span>
                    <span className="text-sm text-muted-foreground">{t("storage.requested")}</span>
                  </div>
                  <dl className="grid grid-cols-2 gap-x-4 gap-y-1.5 text-xs">
                    <dt className="text-muted-foreground">{t("storage.mountPath")}</dt>
                    <dd className="text-right font-mono" translate="no">
                      {m.mountPath}
                    </dd>
                    <dt className="text-muted-foreground">{t("storage.accessMode")}</dt>
                    <dd className="text-right font-mono">{m.accessModes?.join(", ") ?? "—"}</dd>
                    <dt className="text-muted-foreground">{t("storage.class")}</dt>
                    <dd className="text-right font-mono">
                      {m.storageClass ?? t("storage.defaultClass")}
                    </dd>
                  </dl>
                  {editable ? (
                    <Suspense fallback={null}>
                      <ClaimForm
                        mount={m}
                        disabled={!!managed}
                        onExpand={(size) =>
                          planner.plan({
                            title: t("title.expand", name, size),
                            changes: [{ op: "expandVolume", claim: name, size }],
                            service: workload,
                          })
                        }
                        onMount={(newMountPath, readOnly) =>
                          planner.plan({
                            title: t("title.mount", m.mountPath),
                            changes: [
                              {
                                op: "editMount",
                                workload,
                                container: container?.name,
                                mountPath: m.mountPath,
                                newMountPath:
                                  newMountPath === m.mountPath ? undefined : newMountPath,
                                readOnly: readOnly === (m.readOnly ?? false) ? undefined : readOnly,
                              },
                            ],
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
      {!editable ? null : service.kind === "StatefulSet" ? (
        <p className="type-meta text-muted-foreground">{t("form.statefulVolumes")}</p>
      ) : (
        <Suspense fallback={null}>
          <AddVolume
            workload={service.name}
            disabled={!!managed}
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
                <span className="font-mono">{mount.mountPath}</span>
                <span className="text-muted-foreground">{mount.kind}</span>
                <span className="ml-auto font-mono text-muted-foreground">
                  {mount.object ? `${mount.object.kind}/${mount.object.name}` : mount.name}
                </span>
              </li>
            ))}
          </ul>
        </Panel>
      ) : null}
      <VolumeBackups project={project} claims={volumes.map(claimOf)} />
      {planner.dialog}
    </div>
  )
}
