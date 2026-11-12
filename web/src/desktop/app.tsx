import { getCurrentWebview } from "@tauri-apps/api/webview"
import { open as pick } from "@tauri-apps/plugin-dialog"
import { Badge } from "@zeabur-oss/design-system/components/badge"
import { Button } from "@zeabur-oss/design-system/components/button"
import { Checkbox } from "@zeabur-oss/design-system/components/checkbox"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogTitle,
} from "@zeabur-oss/design-system/components/dialog"
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@zeabur-oss/design-system/components/empty"
import { Illustration } from "@zeabur-oss/design-system/components/illustration"
import { Input } from "@zeabur-oss/design-system/components/input"
import { Label } from "@zeabur-oss/design-system/components/label"
import {
  Menu,
  MenuContent,
  MenuItem,
  MenuTrigger,
} from "@zeabur-oss/design-system/components/menu"
import { Select } from "@zeabur-oss/design-system/components/select"
import { Textarea } from "@zeabur-oss/design-system/components/textarea"
import { Tooltip } from "@zeabur-oss/design-system/components/tooltip"
import { useEffect, useMemo, useRef, useState } from "react"
import ClipboardIcon from "~icons/ph/clipboard-text"
import FileIcon from "~icons/ph/file-text"
import FolderIcon from "~icons/ph/folder"
import HouseIcon from "~icons/ph/house"
import MagnifyingGlassIcon from "~icons/ph/magnifying-glass"
import PlusIcon from "~icons/ph/plus"
import StarIcon from "~icons/ph/star"
import StarFillIcon from "~icons/ph/star-fill"
import XIcon from "~icons/ph/x"

import { api, blocked, type Context, type Reach, type Snapshot, type Source } from "./api"
import { languages, useI18n, type Lang, type T } from "./i18n"
import { pasteSample } from "./shots"

function baseName(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean)
  return parts.at(-1) ?? path
}

/** The file a context comes from, as "folder/file" so two `config` files stay apart. */
function fileLabel(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean)
  return parts.slice(-2).join("/")
}

function sourceIcon(kind: Source["kind"]) {
  if (kind === "default") return <HouseIcon className="size-3.5 shrink-0" aria-hidden="true" />
  if (kind === "folder") return <FolderIcon className="size-3.5 shrink-0" aria-hidden="true" />
  if (kind === "pasted") return <ClipboardIcon className="size-3.5 shrink-0" aria-hidden="true" />
  return <FileIcon className="size-3.5 shrink-0" aria-hidden="true" />
}

function ReachBadge({ reach, t }: { reach: Reach | undefined; t: T }) {
  if (!reach) return <Badge variant="idle">{t("reach-checking")}</Badge>
  switch (reach.state) {
    case "reachable":
      return <Badge variant="ok">{`${t("reach-ok")} ${reach.millis} ms`}</Badge>
    case "unresolved":
      return <Badge variant="fail">{t("reach-unresolved")}</Badge>
    case "refused":
      return <Badge variant="fail">{t("reach-refused")}</Badge>
    case "timedOut":
      return <Badge variant="warn">{t("reach-timeout")}</Badge>
    case "noAddress":
      return <Badge variant="fail">{t("reach-none")}</Badge>
  }
}

function authText(context: Context, t: T): string | null {
  const auth = context.auth
  switch (auth.state) {
    case "ok":
      return null
    case "shellPathOnly":
      return t("auth-shell-path", { command: auth.command })
    case "pluginMissing":
      return t("auth-plugin-missing", { command: auth.command })
    case "missingEntry":
      return t(auth.what === "user" ? "auth-missing-user" : "auth-missing-cluster", {
        name: auth.name,
      })
    case "fileMissing":
      return t("auth-file-missing", { path: fileLabel(auth.path) })
  }
}

function parseScenario(text: string | null): Record<string, string> {
  const out: Record<string, string> = {}
  for (const part of (text ?? "").split(";")) {
    const [key, value] = part.split("=")
    if (key) out[key] = value ?? ""
  }
  return out
}

export function App() {
  const { t, lang, setLang } = useI18n()
  const [snap, setSnap] = useState<Snapshot | null>(null)
  const [reach, setReach] = useState<Record<string, Reach>>({})
  const [query, setQuery] = useState("")
  const [only, setOnly] = useState<string | null>(null)
  const [selected, setSelected] = useState<ReadonlySet<string>>(new Set())
  const [paste, setPaste] = useState<{ name: string; yaml: string; error: string | null } | null>(
    null,
  )
  const [dropping, setDropping] = useState(false)
  const nameRef = useRef<HTMLInputElement>(null)

  async function reload() {
    const next = await api.snapshot()
    setSnap(next)
    setReach({})
    for (const c of next.contexts) {
      void api.probe([[c.key, c.server]]).then((results) => {
        for (const r of results) setReach((prev) => ({ ...prev, [r.key]: r.reach }))
      })
    }
    return next
  }

  useEffect(() => {
    void (async () => {
      const loaded = await reload()
      const s = parseScenario(await api.scenario())
      if (s.lang) setLang(s.lang as Lang)
      if (s.q) setQuery(s.q)
      if (s.paste) setPaste({ name: "homelab", yaml: pasteSample, error: null })
      if (s.drop) setDropping(true)
      if (s.only) setOnly(loaded.sources[Number(s.only)]?.id ?? null)
      if (s.select) {
        const names = s.select.split(",")
        setSelected(new Set(loaded.contexts.filter((c) => names.includes(c.name)).map((c) => c.key)))
      }
      if (s.open) {
        const names = s.open.split(",")
        const picked = loaded.contexts.filter((c) => names.includes(c.name))
        void api.open(
          picked.map((c) => [c.key, c.name]),
          s.lang ?? lang,
        )
      }
    })()
    const unlisten = getCurrentWebview().onDragDropEvent((event) => {
      const kind = event.payload.type
      if (kind === "enter" || kind === "over") setDropping(true)
      else if (kind === "leave") setDropping(false)
      else if (kind === "drop") {
        setDropping(false)
        void api.addPaths(event.payload.paths).then(reload)
      }
    })
    return () => {
      void unlisten.then((stop) => stop())
    }
    // Runs once: the scenario and the drop listener belong to the window.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  const visible = useMemo(() => {
    if (!snap) return []
    const q = query.trim().toLowerCase()
    const favourites = new Set(snap.favourites)
    const list = snap.contexts.filter((c) => {
      if (only && c.sourceId !== only) return false
      if (!q) return true
      return [c.name, c.server ?? "", c.user ?? "", c.file, c.cluster].some((v) =>
        v.toLowerCase().includes(q),
      )
    })
    return list.toSorted((a, b) => Number(favourites.has(b.key)) - Number(favourites.has(a.key)))
  }, [snap, query, only])

  if (!snap) return null

  async function addFiles(directory: boolean) {
    const picked = await pick({ multiple: !directory, directory })
    if (!picked) return
    await api.addPaths(Array.isArray(picked) ? picked : [picked])
    await reload()
  }

  const addMenu = (
    <Menu>
      <MenuTrigger
        render={
          <Button variant="secondary" size="sm" className="w-full">
            <PlusIcon aria-hidden="true" />
            {t("add-source")}
          </Button>
        }
      />
      <MenuContent>
        <MenuItem onClick={() => void addFiles(false)}>
          <FileIcon aria-hidden="true" />
          {t("add-files")}
        </MenuItem>
        <MenuItem onClick={() => void addFiles(true)}>
          <FolderIcon aria-hidden="true" />
          {t("add-folder")}
        </MenuItem>
        <MenuItem onClick={() => setPaste({ name: "", yaml: "", error: null })}>
          <ClipboardIcon aria-hidden="true" />
          {t("add-paste")}
        </MenuItem>
      </MenuContent>
    </Menu>
  )

  const languageSelect = (
    <Select
      size="sm"
      aria-label={t("language")}
      items={languages.map((l) => ({ value: l.value, label: l.label, lang: l.value }))}
      value={lang}
      onValueChange={setLang}
    />
  )

  const pasteDialog = paste ? (
    <Dialog open onOpenChange={(next) => (next ? undefined : setPaste(null))}>
      <DialogContent initialFocus={nameRef}>
        <div className="flex items-center gap-2 border-b px-5 py-3">
          <div className="min-w-0 flex-1 truncate">
            <DialogTitle>{t("paste-title")}</DialogTitle>
          </div>
          <DialogClose
            render={<Button variant="ghost" size="icon-sm" aria-label={t("cancel")} />}
          >
            <XIcon aria-hidden="true" />
          </DialogClose>
        </div>
        <form
          className="flex flex-col gap-4 px-5 py-4"
          onSubmit={(event) => {
            event.preventDefault()
            api
              .addPasted(paste.name, paste.yaml)
              .then(() => {
                setPaste(null)
                return reload()
              })
              .catch((error: unknown) => {
                const reason = String(error)
                setPaste({
                  ...paste,
                  error:
                    reason === "no-contexts" ? t("paste-empty") : t("paste-error", { reason }),
                })
              })
          }}
        >
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="paste-name">{t("paste-name")}</Label>
            <Input
              id="paste-name"
              ref={nameRef}
              value={paste.name}
              onChange={(e) => setPaste({ ...paste, name: e.target.value })}
            />
          </div>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="paste-yaml">{t("paste-yaml")}</Label>
            <Textarea
              id="paste-yaml"
              rows={12}
              spellCheck={false}
              className="type-mono"
              value={paste.yaml}
              onChange={(e) => setPaste({ ...paste, yaml: e.target.value, error: null })}
            />
            {paste.error ? <p className="type-meta text-fail-strong">{paste.error}</p> : null}
          </div>
          <div className="flex justify-end gap-2">
            <Button type="button" variant="secondary" onClick={() => setPaste(null)}>
              {t("cancel")}
            </Button>
            <Button type="submit" disabled={!paste.yaml.trim()}>
              {t("paste-add")}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  ) : null

  const dropOverlay = dropping ? (
    <div className="pointer-events-none fixed inset-3 z-50 flex items-center justify-center rounded-xl border-2 border-dashed border-primary bg-surface-page/90">
      <p className="type-heading text-main">{t("drop")}</p>
    </div>
  ) : null

  if (snap.sources.length === 0) {
    return (
      <div className="flex h-dvh flex-col bg-surface-page">
        <div className="flex justify-end p-3">{languageSelect}</div>
        <Empty className="flex-1">
          <EmptyHeader>
            <EmptyMedia>
              <Illustration kind="empty" />
            </EmptyMedia>
            <EmptyTitle>{t("first-title")}</EmptyTitle>
            <EmptyDescription>
              {t("first-body", { path: navigator.userAgent.includes("Windows") ? "%USERPROFILE%\\.kube\\config" : "~/.kube/config" })}
            </EmptyDescription>
          </EmptyHeader>
          <EmptyContent>
            <div className="flex flex-wrap justify-center gap-2">
              <Button onClick={() => void addFiles(false)}>
                <FileIcon aria-hidden="true" />
                {t("add-files")}
              </Button>
              <Button variant="secondary" onClick={() => void addFiles(true)}>
                <FolderIcon aria-hidden="true" />
                {t("add-folder")}
              </Button>
              <Button
                variant="secondary"
                onClick={() => setPaste({ name: "", yaml: "", error: null })}
              >
                <ClipboardIcon aria-hidden="true" />
                {t("add-paste")}
              </Button>
            </div>
            <p className="type-meta text-tertiary">{t("first-drop")}</p>
          </EmptyContent>
        </Empty>
        {pasteDialog}
        {dropOverlay}
      </div>
    )
  }

  const byKey = new Map(snap.contexts.map((c) => [c.key, c]))
  const recents = query || only ? [] : snap.recents.flatMap((k) => byKey.get(k) ?? [])
  const favourites = new Set(snap.favourites)

  function toggle(key: string) {
    const next = new Set(selected)
    if (next.has(key)) next.delete(key)
    else next.add(key)
    setSelected(next)
  }

  function openKeys(keys: readonly string[]) {
    const picked = keys.flatMap((k) => byKey.get(k) ?? [])
    void api.open(
      picked.map((c) => [c.key, c.name]),
      lang,
    ).then(reload)
  }

  const row = (c: Context) => {
    const problem = authText(c, t)
    const isBlocked = blocked(c.auth)
    const fav = favourites.has(c.key)
    return (
      <li
        key={c.key}
        className="grid grid-cols-desktop-row items-center gap-3 border-b border-v2-separator px-4 py-2.5 last:border-b-0"
      >
        <Checkbox
          checked={selected.has(c.key)}
          disabled={isBlocked}
          onCheckedChange={() => toggle(c.key)}
          aria-label={c.name}
        />
        <Tooltip content={fav ? t("favourite-remove") : t("favourite-add")}>
          <Button
            variant="ghost"
            size="icon-xs"
            aria-label={fav ? t("favourite-remove") : t("favourite-add")}
            onClick={() => void api.toggleFavourite(c.key).then(reload)}
          >
            {fav ? (
              <StarFillIcon className="text-warn" aria-hidden="true" />
            ) : (
              <StarIcon aria-hidden="true" />
            )}
          </Button>
        </Tooltip>
        <div className="min-w-0">
          <div className="flex min-w-0 items-center gap-2">
            <span className="truncate type-body font-medium text-main" title={c.name} translate="no">
              {c.name}
            </span>
            {c.current ? <Badge variant="secondary">{t("current")}</Badge> : null}
          </div>
          <div className="flex min-w-0 items-center gap-2 type-meta text-tertiary">
            <span className="shrink-0" title={c.file} translate="no">
              {fileLabel(c.file)}
            </span>
            {c.sameNameElsewhere > 0 ? (
              <span
                className="min-w-0 truncate text-warn-strong"
                title={t("same-name", { n: c.sameNameElsewhere })}
              >
                {t("same-name", { n: c.sameNameElsewhere })}
              </span>
            ) : null}
          </div>
        </div>
        <div className="min-w-0">
          <div className="truncate type-mono type-meta text-secondary" title={c.server ?? ""} translate="no">
            {c.server ?? "-"}
          </div>
          <div className="truncate type-meta text-tertiary" title={c.user ?? ""} translate="no">
            {c.user ?? "-"}
          </div>
        </div>
        <div className="flex min-w-0 flex-col items-start gap-1">
          <ReachBadge reach={reach[c.key]} t={t} />
          {problem ? (
            <span
              className={
                isBlocked
                  ? "max-w-full truncate type-meta text-fail-strong"
                  : "max-w-full truncate type-meta text-warn-strong"
              }
              title={problem}
            >
              {problem}
            </span>
          ) : null}
        </div>
        <Button variant="secondary" size="xs" disabled={isBlocked} onClick={() => openKeys([c.key])}>
          {t("open")}
        </Button>
      </li>
    )
  }

  return (
    <div className="flex h-dvh bg-surface-page text-main">
      <aside className="flex w-64 shrink-0 flex-col gap-3 border-r border-v2-border bg-sidebar p-3">
        <p className="px-2 pt-1 type-label text-tertiary">{t("sources")}</p>
        <nav className="flex flex-col gap-0.5">
          <button
            type="button"
            className={
              only === null
                ? "flex items-center gap-2 rounded-md bg-v2-surface-active px-2 py-1.5 text-left type-body text-main"
                : "flex items-center gap-2 rounded-md px-2 py-1.5 text-left type-body text-secondary hover:bg-v2-surface-hover"
            }
            onClick={() => setOnly(null)}
          >
            <span className="flex-1 truncate">{t("source-all")}</span>
            <span className="type-meta text-tertiary">{snap.contexts.length}</span>
          </button>
          {snap.sources.map((s) => (
            <div
              key={s.id}
              className={
                only === s.id
                  ? "group flex items-start gap-2 rounded-md bg-v2-surface-active px-2 py-1.5"
                  : "group flex items-start gap-2 rounded-md px-2 py-1.5 hover:bg-v2-surface-hover"
              }
            >
              <button
                type="button"
                className="flex min-w-0 flex-1 items-start gap-2 text-left"
                onClick={() => setOnly(s.id)}
                title={s.path}
              >
                <span className="mt-0.5 text-tertiary">{sourceIcon(s.kind)}</span>
                <span className="min-w-0 flex-1">
                  <span className="block truncate type-body text-main" translate="no">
                    {baseName(s.path)}
                  </span>
                  <span className="block truncate type-meta text-tertiary">
                    {s.kind === "default"
                      ? t("source-default")
                      : s.kind === "folder"
                        ? t("source-folder")
                        : s.kind === "pasted"
                          ? t("source-pasted")
                          : t("source-file")}
                  </span>
                  {s.missing ? (
                    <span className="block type-meta text-fail-strong">{t("source-missing")}</span>
                  ) : null}
                  {s.errors.length > 0 ? (
                    <span
                      className="block type-meta text-warn-strong"
                      title={s.errors.map((e) => `${e.file}: ${e.reason}`).join("\n")}
                    >
                      {t("source-errors", { n: s.errors.length })}
                    </span>
                  ) : null}
                </span>
                <span className="type-meta text-tertiary">{s.contexts}</span>
              </button>
              <Button
                variant="ghost"
                size="icon-xs"
                aria-label={t("source-remove", { name: baseName(s.path) })}
                onClick={() => void api.removeSource(s.id).then(reload)}
              >
                <XIcon aria-hidden="true" />
              </Button>
            </div>
          ))}
        </nav>
        <div className="px-2">{addMenu}</div>
        <div className="mt-auto px-2">{languageSelect}</div>
      </aside>

      <main className="flex min-w-0 flex-1 flex-col">
        <header className="flex flex-col gap-3 border-b border-v2-border px-5 pt-4 pb-3">
          <div className="flex items-center gap-3">
            <h1 className="type-title">{t("title")}</h1>
            <span className="type-meta text-tertiary">{t("count", { n: visible.length })}</span>
            {selected.size > 0 ? (
              <div className="ml-auto flex items-center gap-2">
                <span className="type-meta text-secondary">{t("selected", { n: selected.size })}</span>
                <Button variant="ghost" size="sm" onClick={() => setSelected(new Set())}>
                  {t("clear")}
                </Button>
                <Button size="sm" onClick={() => openKeys([...selected])}>
                  {t("open-n", { n: selected.size })}
                </Button>
              </div>
            ) : null}
          </div>
          <div className="relative">
            <MagnifyingGlassIcon
              className="pointer-events-none absolute top-1/2 left-3 size-3.5 -translate-y-1/2 text-tertiary"
              aria-hidden="true"
            />
            <Input
              className="pl-8"
              placeholder={t("search")}
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              aria-label={t("search")}
            />
          </div>
        </header>
        <div className="flex-1 overflow-y-auto px-5 py-4">
          {recents.length > 0 ? (
            <section className="mb-5">
              <p className="mb-2 type-label text-tertiary">{t("recent")}</p>
              <ul className="rounded-lg border border-v2-border bg-surface">{recents.map(row)}</ul>
            </section>
          ) : null}
          {visible.length === 0 ? (
            <p className="py-10 text-center type-body text-secondary">{t("no-results", { q: query })}</p>
          ) : (
            <ul className="rounded-lg border border-v2-border bg-surface">{visible.map(row)}</ul>
          )}
          <p className="mt-3 type-meta text-tertiary">{t("reach-note")}</p>
        </div>
      </main>
      {pasteDialog}
      {dropOverlay}
    </div>
  )
}
