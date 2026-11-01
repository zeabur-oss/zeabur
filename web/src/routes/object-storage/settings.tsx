import { useState } from "react"

import { AmountInput } from "@/components/amount-input"
import { FormField } from "@/components/form-field"
import { GateNote, GateTip, useGate } from "@/components/gate"
import { usePlanner } from "@/components/plan/planner"
import { kindWord, useGrowth } from "@/components/storage-class"
import { ObjectYaml } from "@/components/store-detail"
import { useT } from "@/i18n"
import { useQuantityFormat } from "@/lib/quantity"
import { storageProblem } from "@/model/databases"
import { growObjectStorage, storeAddress, type ObjectStorage } from "@/model/object-storage"
import type { ServiceMode } from "@/routes/search"
import { ModeSwitch } from "@/routes/service/views"
import { Button } from "@zeabur-oss/design-system/components/button"
import { SectionHeader } from "@zeabur-oss/design-system/components/section-header"

/** The volume servers' disk; or, in its YAML view, the operator's object. */
export function ObjectStorageSettings({
  project,
  store,
  mode,
  onMode,
}: {
  project: string
  store: ObjectStorage
  mode?: ServiceMode
  onMode: (mode?: ServiceMode) => void
}) {
  return (
    <div className="flex flex-col gap-6">
      <ModeSwitch tab="settings" mode={mode} onMode={onMode} />
      {mode === "yaml" ? (
        <ObjectYaml project={project} address={storeAddress(store)} />
      ) : (
        <Storage key={store.storage} project={project} store={store} />
      )}
    </div>
  )
}

function Storage({ project, store }: { project: string; store: ObjectStorage }) {
  const t = useT()
  const { amount } = useQuantityFormat()
  const gate = useGate(project, "changeObjectStorage")
  const planner = usePlanner(project)
  const [storage, setStorage] = useState(store.storage ?? "")
  const [tried, setTried] = useState(false)
  const { storageClass, fixed, description } = useGrowth(project, store.storageClass)
  const problem = storageProblem(store.storage, storage)
  const changed = storage.trim() !== (store.storage ?? "")
  const review = () => {
    setTried(true)
    if (problem) return
    planner.plan({
      title: t("s3.grow", store.name),
      changes: growObjectStorage(store, storage),
      summary: {
        lines: [
          t(
            "s3.growSentence",
            store.name,
            store.storage ? amount(store.storage, "bytes") : "—",
            amount(storage.trim(), "bytes"),
          ),
        ],
        action: t("action.grow", amount(storage.trim(), "bytes")),
      },
    })
  }
  return (
    <form
      className="flex min-w-0 flex-col gap-3"
      onSubmit={(event) => {
        event.preventDefault()
        review()
      }}
    >
      <SectionHeader id="storage-title" title={t("part.storage")} description={description} />
      <GateNote gate={gate} />
      <div className="flex flex-col gap-4 rounded-xl border border-v2-border bg-card p-5">
        <div className="grid grid-cols-1 items-start gap-4 sm:grid-cols-2">
          <FormField
            label={t("s3.field.storage")}
            hint={
              storageClass
                ? t("storage.option", kindWord(t, storageClass), storageClass.name, false)
                : undefined
            }
            errors={
              tried && problem
                ? [
                    problem === "shrink"
                      ? t("db.storage.shrink", amount(store.storage, "bytes"))
                      : t("db.storage.invalid"),
                  ]
                : []
            }
          >
            <AmountInput
              dimension="bytes"
              value={storage}
              disabled={!gate.open || fixed}
              onChange={setStorage}
            />
          </FormField>
        </div>
      </div>
      <div className="flex justify-end">
        <GateTip gate={gate}>
          <Button size="sm" type="submit" disabled={!changed}>
            {t("edit.review")}
          </Button>
        </GateTip>
      </div>
      {planner.dialog}
    </form>
  )
}
