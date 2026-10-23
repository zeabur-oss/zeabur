import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import { useState } from "react"
import WarningIcon from "~icons/ph/warning"

import type { Schemas } from "@/api/client"
import { objectStorageQuery } from "@/api/queries"
import { ConnectFields, useNewConnections } from "@/components/connect"
import { NameField, quantity } from "@/components/form-field"
import { useGate } from "@/components/gate"
import { OpenButton } from "@/components/store-detail"
import { PlanDrawer, ReviewFooter, ReviewForm } from "@/components/plan/plan-drawer"
import { usePlanStep } from "@/components/plan/planner"
import { StatusPill } from "@/components/status"
import { StorageClassField, StorageSizeField, useStorage } from "@/components/storage-class"
import { useT } from "@/i18n"
import { objectStorageLink } from "@/lib/place"
import { nameProblem, stateHealth } from "@/model/databases"
import {
  bucketProblem,
  createObjectStorage,
  newObjectStorage,
  pendingObjectStorage,
  type NewObjectStorage,
  type ObjectStorage,
} from "@/model/object-storage"
import { chosenClass } from "@/model/storage"
import { Alert, AlertDescription } from "@zeabur/design-system/components/alert"

type Data = Schemas["ProjectObjectStorage"]

export const useObjectStorage = (project: string) => useQuery(objectStorageQuery(project))

export function ObjectStorageState({
  store,
  size = "sm",
}: {
  store: ObjectStorage
  size?: "sm" | "md"
}) {
  const t = useT()
  return (
    <StatusPill health={stateHealth[store.state]} size={size}>
      {store.state === "ready"
        ? t(`db.state.${store.state}`)
        : `${t(`db.state.${store.state}`)}${t("common.sep")}${t("db.membersReady", store.ready, store.members)}`}
    </StatusPill>
  )
}

export function ObjectStorageRow({ project, store }: { project: string; store: ObjectStorage }) {
  const t = useT()
  return (
    <li className="flex items-center gap-2.5 border-t border-rule py-2.75">
      <div className="flex min-w-0 flex-1 flex-col">
        <Link
          {...objectStorageLink(project, store)}
          className="truncate type-mono hover:underline"
          translate="no"
        >
          {store.name}
        </Link>
        <span className="truncate type-meta text-muted-foreground">
          {t("s3.engine")}
          {t("common.sep")}
          {t("s3.bucketCount", store.buckets.length)}
        </span>
      </div>
      <ObjectStorageState store={store} />
    </li>
  )
}

/** Why stores cannot be listed or created: no operator, or no right to read them. */
export function ObjectStorageNotes({ data }: { data: Data }) {
  const t = useT()
  if (data.state === "refused")
    return (
      <p className="border-t border-rule py-2.5 type-meta text-muted-foreground">
        {t("s3.refused")}
      </p>
    )
  if (data.state !== "not-installed") return null
  return (
    <p className="border-t border-rule py-2.5 type-meta text-muted-foreground">
      {t("s3.noOperator")}{" "}
      <Link to="/cluster/components" className="text-foreground hover:underline">
        {t("backups.seeComponents")}
      </Link>
    </p>
  )
}

/** "New object storage", and its form, when the operator is installed. */
export function NewObjectStorageButton({
  project,
  data,
  size = "xs",
}: {
  project: string
  data: Data
  size?: "xs" | "sm"
}) {
  const t = useT()
  const gate = useGate(project, "changeObjectStorage")
  if (data.state !== "installed") return null
  return (
    <OpenButton
      gate={gate}
      label={t("s3.new")}
      size={size}
      form={(close) => (
        <NewObjectStorageDrawer
          project={project}
          taken={data.items.map((s) => s.name)}
          onClose={close}
        />
      )}
    />
  )
}

export function NewObjectStorageDrawer({
  project,
  taken,
  onClose,
}: {
  project: string
  taken: string[]
  onClose: () => void
}) {
  const t = useT()
  const [form, setForm] = useState<NewObjectStorage>(newObjectStorage)
  const [bucket, setBucket] = useState("uploads")
  const [tried, setTried] = useState(false)
  const step = usePlanStep(project, onClose)
  const storage = useStorage(project)
  const set = (patch: Partial<NewObjectStorage>) => setForm((f) => ({ ...f, ...patch }))
  const buckets = bucket ? [bucket] : []
  const full = { ...form, storage: form.storage.trim(), buckets }
  const connections = useNewConnections(project, pendingObjectStorage(full, project))
  const nameIssue = nameProblem(form.name, taken)
  const bucketIssue = bucket ? bucketProblem(bucket, []) : undefined
  const storageBad = !quantity.test(full.storage)
  const review = () => {
    setTried(true)
    if (nameIssue || bucketIssue || storageBad || connections.problem) return
    step.plan({
      title: t("s3.planCreate", form.name),
      changes: [...createObjectStorage(full), ...connections.changes()],
      notes: connections.notes(),
    })
  }
  return (
    <PlanDrawer
      step={step}
      onClose={onClose}
      title={t("s3.new")}
      description={t("s3.newBody")}
      footer={<ReviewFooter onCancel={onClose} onReview={review} />}
    >
      <ReviewForm onReview={review}>
        <NameField
          label={t("db.field.name")}
          error={tried && nameIssue ? t(`s3.name.${nameIssue}`) : undefined}
          value={form.name}
          onChange={(name) => set({ name })}
        />
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
          <StorageSizeField
            label={t("s3.field.storage")}
            storage={storage}
            storageClass={form.storageClass}
            value={form.storage}
            bad={tried && storageBad}
            onChange={(value) => set({ storage: value })}
          />
          <NameField
            label={t("s3.field.bucket")}
            hint={t("s3.bucketHint")}
            error={tried && bucketIssue ? t(`s3.bucket.${bucketIssue}`) : undefined}
            value={bucket}
            onChange={setBucket}
          />
        </div>
        <StorageClassField
          storage={storage}
          value={form.storageClass}
          onChange={(name) => set({ storageClass: chosenClass(storage, name) })}
        />
        <Alert variant="warn">
          <WarningIcon aria-hidden="true" />
          <AlertDescription>{t("s3.oneCopy")}</AlertDescription>
        </Alert>
        <ConnectFields {...connections.fieldProps} optional tried={tried} />
      </ReviewForm>
    </PlanDrawer>
  )
}
