import { Button } from "@zeabur-oss/design-system/components/button"
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@zeabur-oss/design-system/components/empty"
import { Illustration } from "@zeabur-oss/design-system/components/illustration"
import { Menu, MenuContent, MenuItem, MenuTrigger } from "@zeabur-oss/design-system/components/menu"
import { Select } from "@zeabur-oss/design-system/components/select"
import ClipboardIcon from "~icons/ph/clipboard-text"
import FileIcon from "~icons/ph/file-text"
import FolderIcon from "~icons/ph/folder"
import PlusIcon from "~icons/ph/plus"

import { localeNames, locales, useI18n } from "@/i18n"

/** What the add buttons and menu do; the window supplies it. */
export interface AddActions {
  addFiles: (directory: boolean) => void
  newPaste: () => void
}

export function LanguageSelect() {
  const { t, locale, setLocale } = useI18n()
  return (
    <Select
      size="sm"
      aria-label={t("desktop.language")}
      items={locales.map((l) => ({ value: l, label: localeNames[l], lang: l }))}
      value={locale}
      onValueChange={setLocale}
    />
  )
}

export function AddMenu({ actions }: { actions: AddActions }) {
  const { t } = useI18n()
  return (
    <Menu>
      <MenuTrigger
        render={
          <Button variant="secondary" size="sm" className="w-full">
            <PlusIcon aria-hidden="true" />
            {t("desktop.addSource")}
          </Button>
        }
      />
      <MenuContent>
        <MenuItem onClick={() => actions.addFiles(false)}>
          <FileIcon aria-hidden="true" />
          {t("desktop.addFiles")}
        </MenuItem>
        <MenuItem onClick={() => actions.addFiles(true)}>
          <FolderIcon aria-hidden="true" />
          {t("desktop.addFolder")}
        </MenuItem>
        <MenuItem onClick={actions.newPaste}>
          <ClipboardIcon aria-hidden="true" />
          {t("desktop.addPaste")}
        </MenuItem>
      </MenuContent>
    </Menu>
  )
}

export function DropOverlay({ dropping }: { dropping: boolean }) {
  const { t } = useI18n()
  if (!dropping) return null
  return (
    <div className="pointer-events-none fixed inset-3 z-50 flex items-center justify-center rounded-xl border-2 border-dashed border-primary bg-surface-page/90">
      <p className="type-heading text-main">{t("desktop.drop")}</p>
    </div>
  )
}

export function FirstRun({ actions }: { actions: AddActions }) {
  const { t } = useI18n()
  const path = navigator.userAgent.includes("Windows")
    ? "%USERPROFILE%\\.kube\\config"
    : "~/.kube/config"
  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <div className="flex justify-end p-3">
        <LanguageSelect />
      </div>
      <Empty className="flex-1">
        <EmptyHeader>
          <EmptyMedia>
            <Illustration kind="empty" />
          </EmptyMedia>
          <EmptyTitle>{t("desktop.firstTitle")}</EmptyTitle>
          <EmptyDescription>{t("desktop.firstBody", path)}</EmptyDescription>
        </EmptyHeader>
        <EmptyContent>
          <div className="flex flex-wrap justify-center gap-2">
            <Button onClick={() => actions.addFiles(false)}>
              <FileIcon aria-hidden="true" />
              {t("desktop.addFiles")}
            </Button>
            <Button variant="secondary" onClick={() => actions.addFiles(true)}>
              <FolderIcon aria-hidden="true" />
              {t("desktop.addFolder")}
            </Button>
            <Button variant="secondary" onClick={actions.newPaste}>
              <ClipboardIcon aria-hidden="true" />
              {t("desktop.addPaste")}
            </Button>
          </div>
          <p className="type-meta text-tertiary">{t("desktop.firstDrop")}</p>
        </EmptyContent>
      </Empty>
    </div>
  )
}
