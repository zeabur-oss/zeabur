import { open as pick } from "@tauri-apps/plugin-dialog"
import { useMemo, useState } from "react"

import { useI18n } from "@/i18n"

import { api, type Context, type Snapshot } from "./api"
import { DropOverlay, FirstRun, type AddActions } from "./chrome"
import { ContextPane, type RowActions } from "./contexts"
import { useDropTarget, useScenario, useSnapshot } from "./hooks"
import { recentContexts, sourceFilter, visibleContexts } from "./model"
import { PasteDialog, type Paste } from "./paste-dialog"
import { pasteSample } from "./shots"
import { Sidebar } from "./sources"

interface BrowserProps {
  snap: Snapshot
  reach: ReturnType<typeof useSnapshot>["reach"]
  query: string
  setQuery: (query: string) => void
  only: string | null
  setOnly: (id: string | null) => void
  add: AddActions
  reload: () => Promise<unknown>
}

function Browser({
  snap,
  reach,
  query,
  setQuery,
  only: chosen,
  setOnly,
  add,
  reload,
}: BrowserProps) {
  const { locale } = useI18n()
  const only = sourceFilter(snap, chosen)
  const visible = useMemo(() => visibleContexts(snap, query, only), [snap, query, only])
  const rows: RowActions = {
    open: (c: Context) => void api.open(c.key, c.name, locale).then(reload),
    toggleFavourite: (c: Context) => void api.toggleFavourite(c.key).then(reload),
  }
  return (
    <>
      <Sidebar
        sources={snap.sources}
        total={snap.contexts.length}
        only={only}
        setOnly={setOnly}
        removeSource={(id) => void api.removeSource(id).then(reload)}
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
  const { locale, setLocale } = useI18n()
  const { snap, reach, reload } = useSnapshot()
  const [query, setQuery] = useState("")
  const [only, setOnly] = useState<string | null>(null)
  const [paste, setPaste] = useState<Paste | null>(null)
  const { dropping, setDropping } = useDropTarget(reload)

  useScenario(reload, {
    locale,
    setLocale,
    setQuery,
    setOnly,
    showPaste: () => setPaste({ name: "homelab", yaml: pasteSample, error: null }),
    showDrop: () => setDropping(true),
  })

  if (!snap) return null

  const add: AddActions = {
    addFiles: (directory) => void addFiles(directory),
    newPaste: () => setPaste({ name: "", yaml: "", error: null }),
  }

  async function addFiles(directory: boolean) {
    const picked = await pick({ multiple: !directory, directory })
    if (!picked) return
    await api.addPaths(Array.isArray(picked) ? picked : [picked])
    await reload()
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
      reload={reload}
    />
  )

  return (
    <div className="flex h-dvh bg-surface-page text-main">
      {snap.sources.length === 0 ? <FirstRun actions={add} /> : browser}
      <PasteDialog paste={paste} setPaste={setPaste} reload={reload} />
      <DropOverlay dropping={dropping} />
    </div>
  )
}
