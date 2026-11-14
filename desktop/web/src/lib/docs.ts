import type { Locale } from "@/i18n"

/** A page of Zeabur's docs on oss.zeabur.com, in the reader's language. */
export function docsUrl(locale: Locale, page: string): string {
  const prefix = locale === "en" ? "" : `/${locale}`
  return `https://oss.zeabur.com${prefix}/docs/zeabur/${page}/`
}
