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
import { Select } from "@zeabur-oss/design-system/components/select"
import FileIcon from "~icons/ph/file-text"
import type { ReactElement } from "react"

import { localeNames, locales, useI18n } from "@/i18n"

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

export function FirstRun({ addFiles, update }: { addFiles: () => void; update: ReactElement }) {
  const { t } = useI18n()
  const path = navigator.userAgent.includes("Windows")
    ? "%USERPROFILE%\\.kube\\config"
    : "~/.kube/config"
  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <div className="flex items-center justify-end gap-2 p-3">
        <div>{update}</div>
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
          <Button onClick={addFiles}>
            <FileIcon aria-hidden="true" />
            {t("desktop.addFiles")}
          </Button>
        </EmptyContent>
      </Empty>
    </div>
  )
}
