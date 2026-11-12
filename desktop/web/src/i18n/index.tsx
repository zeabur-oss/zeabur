import { createContext, use, useEffect, useState, type ReactNode } from "react"

import { en, type Messages } from "@/i18n/en"
import { zhCN } from "@/i18n/zh-CN"
import { zhTW } from "@/i18n/zh-TW"

export const locales = ["en", "zh-TW", "zh-CN"] as const
export type Locale = (typeof locales)[number]

export const localeNames: Record<Locale, string> = {
  en: "English",
  "zh-TW": "繁體中文（台灣）",
  "zh-CN": "简体中文",
}

const catalogues: Record<Locale, Messages> = { en, "zh-TW": zhTW, "zh-CN": zhCN }

// The same key as the web interface, so a cluster window opens in the chooser's language.
const storageKey = "zeabur.locale"

/** Maps a BCP 47 tag to a supported locale, or `undefined`. */
function matchLocale(tag: string): Locale | undefined {
  const lower = tag.toLowerCase()
  if (lower === "zh-tw" || lower === "zh-hk" || lower === "zh-mo" || lower.startsWith("zh-hant"))
    return "zh-TW"
  if (lower === "zh" || lower.startsWith("zh-")) return "zh-CN"
  if (lower === "en" || lower.startsWith("en-")) return "en"
  return undefined
}

function savedLocale(): Locale | undefined {
  try {
    const saved = window.localStorage.getItem(storageKey)
    return locales.find((locale) => locale === saved)
  } catch {
    return undefined
  }
}

function saveLocale(locale: Locale) {
  try {
    window.localStorage.setItem(storageKey, locale)
  } catch {
    // Storage may be unavailable; the choice lasts until the window closes.
  }
}

function initialLocale(): Locale {
  const saved = savedLocale()
  if (saved) return saved
  for (const tag of navigator.languages ?? [navigator.language]) {
    const match = matchLocale(tag)
    if (match) return match
  }
  return "en"
}

type Key = keyof Messages
type Args<K extends Key> = Messages[K] extends (...args: infer A) => string ? A : []
export type Translate = <K extends Key>(key: K, ...args: Args<K>) => string

function translator(messages: Messages): Translate {
  return (key, ...args) => {
    const message = messages[key] as string | ((...a: unknown[]) => string)
    return typeof message === "function" ? message(...args) : message
  }
}

interface I18n {
  locale: Locale
  setLocale: (locale: Locale) => void
  t: Translate
}

const I18nContext = createContext<I18n>({
  locale: "en",
  setLocale: () => {},
  t: translator(en),
})

export function I18nProvider({ children }: { children: ReactNode }) {
  const [locale, setLocaleState] = useState<Locale>(initialLocale)

  useEffect(() => {
    document.documentElement.lang = locale
  }, [locale])

  const setLocale = (next: Locale) => {
    saveLocale(next)
    setLocaleState(next)
  }

  return (
    <I18nContext value={{ locale, setLocale, t: translator(catalogues[locale]) }}>
      {children}
    </I18nContext>
  )
}

export function useI18n(): I18n {
  return use(I18nContext)
}
