import { Button } from "@zeabur-oss/design-system/components/button"
import ClipboardIcon from "~icons/ph/clipboard-text"
import FileIcon from "~icons/ph/file-text"
import FolderIcon from "~icons/ph/folder"
import HouseIcon from "~icons/ph/house"
import XIcon from "~icons/ph/x"

import { useI18n } from "@/i18n"

import type { Source, SourceKind } from "./api"
import { AddMenu, LanguageSelect, type AddActions } from "./chrome"
import { baseName, sourceKindKeys } from "./model"

const iconClass = "size-3.5 shrink-0"

const sourceIcons: Record<SourceKind, () => React.ReactElement> = {
  default: () => <HouseIcon className={iconClass} aria-hidden="true" />,
  folder: () => <FolderIcon className={iconClass} aria-hidden="true" />,
  pasted: () => <ClipboardIcon className={iconClass} aria-hidden="true" />,
  file: () => <FileIcon className={iconClass} aria-hidden="true" />,
}

const rowBase = "flex items-center gap-2 rounded-md px-2 py-1.5 text-left type-body"

function SourceNotes({ source }: { source: Source }) {
  const { t } = useI18n()
  const failed = source.errors.map((e) => `${e.file}: ${e.reason}`).join("\n")
  return (
    <>
      {source.missing ? (
        <span className="block type-meta text-fail-strong">{t("desktop.sourceMissing")}</span>
      ) : null}
      {source.errors.length > 0 ? (
        <span className="block type-meta text-warn-strong" title={failed}>
          {t("desktop.sourceErrors", source.errors.length)}
        </span>
      ) : null}
    </>
  )
}

interface ItemProps {
  source: Source
  active: boolean
  onSelect: () => void
  onRemove: () => void
}

function SourceItem({ source, active, onSelect, onRemove }: ItemProps) {
  const { t } = useI18n()
  const tone = active ? "bg-v2-surface-active" : "hover:bg-v2-surface-hover"
  return (
    <div className={`group flex items-start gap-2 rounded-md px-2 py-1 ${tone}`}>
      <button
        type="button"
        className="flex min-w-0 flex-1 items-start gap-2 text-left"
        onClick={onSelect}
        title={source.path}
      >
        <span className="flex h-control-xs items-center text-tertiary">{sourceIcons[source.kind]()}</span>
        <span className="min-w-0 flex-1">
          <span className="flex h-control-xs items-center gap-2">
            <span className="min-w-0 flex-1 truncate type-body text-main" translate="no">
              {baseName(source.path)}
            </span>
            <span className="type-meta text-tertiary tabular-nums">{source.contexts}</span>
          </span>
          <span className="block truncate type-meta text-tertiary">
            {t(sourceKindKeys[source.kind])}
          </span>
          <SourceNotes source={source} />
        </span>
      </button>
      <Button
        variant="ghost"
        size="icon-xs"
        aria-label={t("desktop.sourceRemove", baseName(source.path))}
        onClick={onRemove}
      >
        <XIcon aria-hidden="true" />
      </Button>
    </div>
  )
}

interface Props {
  sources: Source[]
  total: number
  only: string | null
  setOnly: (id: string | null) => void
  removeSource: (id: string) => void
  add: AddActions
}

export function Sidebar({ sources, total, only, setOnly, removeSource, add }: Props) {
  const { t } = useI18n()
  const allTone =
    only === null ? "bg-v2-surface-active text-main" : "text-secondary hover:bg-v2-surface-hover"
  return (
    <aside className="flex w-64 shrink-0 flex-col gap-3 border-r border-v2-border bg-sidebar p-3">
      <p className="px-2 pt-1 type-label text-tertiary">{t("desktop.sources")}</p>
      <nav className="flex flex-col gap-0.5">
        <button type="button" className={`${rowBase} ${allTone}`} onClick={() => setOnly(null)}>
          <span className="flex-1 truncate">{t("desktop.sourceAll")}</span>
          <span className="type-meta text-tertiary">{total}</span>
        </button>
        {sources.map((s) => (
          <SourceItem
            key={s.id}
            source={s}
            active={only === s.id}
            onSelect={() => setOnly(s.id)}
            onRemove={() => removeSource(s.id)}
          />
        ))}
      </nav>
      <div className="px-2">
        <AddMenu actions={add} />
      </div>
      <div className="mt-auto px-2">
        <LanguageSelect />
      </div>
    </aside>
  )
}
