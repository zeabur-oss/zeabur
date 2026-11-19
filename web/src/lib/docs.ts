import type { Locale } from "@/i18n"

/** A page of Zeabur's docs on oss.zeabur.com, in the reader's language, at a heading's anchor. */
export function docsUrl(locale: Locale, page: string, anchor?: string): string {
  const prefix = locale === "en" ? "" : `/${locale}`
  const hash = anchor ? `#${encodeURIComponent(anchor)}` : ""
  return `https://oss.zeabur.com${prefix}/docs/zeabur/${page}/${hash}`
}
