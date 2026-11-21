import { open as pick } from "@tauri-apps/plugin-dialog"
import { useMemo, useState } from "react"

import { useI18n } from "@/i18n"

import type { Context, Snapshot } from "./api"
import { FirstRun } from "./chrome"
import { ContextPane, type RowActions } from "./contexts"
import { useChanges, useSnapshot } from "./hooks"
import { useMenuBar } from "./menu"
import { sourceFilter, visibleContexts } from "./model"
import { Sidebar } from "./sources"
import { UpdateDialog } from "./update"

interface BrowserProps {
  snap: Snapshot
  query: string
  setQuery: (query: string) => void
  only: string | null
  setOnly: (id: string | null) => void
  addFiles: () => void
}

function Browser({ snap, query, setQuery, only: chosen, setOnly, addFiles }: BrowserProps) {
  const { locale } = useI18n()
  const only = sourceFilter(snap, chosen)
  const visible = useMemo(() => visibleContexts(snap, query, only), [snap, query, only])
  const changes = useChanges()
  const rows: RowActions = {
    open: (c: Context) => changes.open.mutate({ key: c.key, name: c.name, lang: locale }),
  }
  return (
    <>
      <Sidebar
        sources={snap.sources}
        total={snap.contexts.length}
        only={only}
        setOnly={setOnly}
        removeSource={(id) => changes.removeSource.mutate(id)}
        addFiles={addFiles}
      />
      <ContextPane
        visible={visible}
        shell={snap.shell}
        actions={rows}
        query={query}
        setQuery={setQuery}
      />
    </>
  )
}

export function App() {
  const snap = useSnapshot()
  const { addPaths } = useChanges()
  const [query, setQuery] = useState("")
  const [only, setOnly] = useState<string | null>(null)
  useMenuBar()

  if (!snap) return null

  async function addFiles() {
    const picked = await pick({ multiple: true })
    if (!picked) return
    addPaths.mutate(picked)
  }

  return (
    <div data-titlebar-inset className="flex h-dvh bg-surface-page text-main select-none">
      {snap.sources.length === 0 ? (
        <FirstRun addFiles={() => void addFiles()} />
      ) : (
        <Browser
          snap={snap}
          query={query}
          setQuery={setQuery}
          only={only}
          setOnly={setOnly}
          addFiles={() => void addFiles()}
        />
      )}
      <UpdateDialog />
    </div>
  )
}
