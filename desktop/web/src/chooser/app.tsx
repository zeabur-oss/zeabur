import { open as pick } from "@tauri-apps/plugin-dialog"
import { useMemo, useState } from "react"

import { useI18n } from "@/i18n"

import type { Context, Snapshot } from "./api"
import { DropOverlay, FirstRun, type AddActions } from "./chrome"
import { ContextPane, type RowActions } from "./contexts"
import { useChanges, useDropTarget, useSnapshot } from "./hooks"
import { useMenuBar } from "./menu"
import { recentContexts, sourceFilter, visibleContexts } from "./model"
import { PasteDialog, type Paste } from "./paste-dialog"
import { Sidebar } from "./sources"

interface BrowserProps {
  snap: Snapshot
  reach: ReturnType<typeof useSnapshot>["reach"]
  query: string
  setQuery: (query: string) => void
  only: string | null
  setOnly: (id: string | null) => void
  add: AddActions
}

function Browser({ snap, reach, query, setQuery, only: chosen, setOnly, add }: BrowserProps) {
  const { locale } = useI18n()
  const only = sourceFilter(snap, chosen)
  const visible = useMemo(() => visibleContexts(snap, query, only), [snap, query, only])
  const changes = useChanges()
  const rows: RowActions = {
    open: (c: Context) => changes.open.mutate({ key: c.key, name: c.name, lang: locale }),
    toggleFavourite: (c: Context) => changes.toggleFavourite.mutate(c.key),
  }
  return (
    <>
      <Sidebar
        sources={snap.sources}
        total={snap.contexts.length}
        only={only}
        setOnly={setOnly}
        removeSource={(id) => changes.removeSource.mutate(id)}
        add={add}
      />
      <ContextPane
        visible={visible}
        recents={recentContexts(snap, query, only)}
        favourites={new Set(snap.favourites)}
        reach={reach}
        shell={snap.shell}
        actions={rows}
        query={query}
        setQuery={setQuery}
      />
    </>
  )
}

export function App() {
  const { snap, reach } = useSnapshot()
  const { addPaths } = useChanges()
  const [query, setQuery] = useState("")
  const [only, setOnly] = useState<string | null>(null)
  const [paste, setPaste] = useState<Paste | null>(null)
  const { dropping } = useDropTarget()
  useMenuBar()

  if (!snap) return null

  const add: AddActions = {
    addFiles: (directory) => void addFiles(directory),
    newPaste: () => setPaste({ name: "", yaml: "", error: null }),
  }

  async function addFiles(directory: boolean) {
    const picked = await pick({ multiple: !directory, directory })
    if (!picked) return
    addPaths.mutate(Array.isArray(picked) ? picked : [picked])
  }

  const browser = (
    <Browser
      snap={snap}
      reach={reach}
      query={query}
      setQuery={setQuery}
      only={only}
      setOnly={setOnly}
      add={add}
    />
  )

  return (
    <div data-titlebar-inset className="flex h-dvh bg-surface-page text-main select-none">
      {snap.sources.length === 0 ? <FirstRun actions={add} /> : browser}
      <PasteDialog paste={paste} setPaste={setPaste} />
      <DropOverlay dropping={dropping} />
    </div>
  )
}
