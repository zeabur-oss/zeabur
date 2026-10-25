import { createContext, use, useEffect, useState, type ReactNode } from "react"
import * as z from "zod/mini"

import { core } from "@/i18n/en-core"
import type { Messages } from "@/i18n/en"

export const locales = ["en", "zh-TW", "zh-CN"] as const
export type Locale = (typeof locales)[number]

export const localeNames: Record<Locale, string> = {
  en: "English",
  "zh-TW": "繁體中文（台灣）",
  "zh-CN": "简体中文",
}

type Loader = () => Promise<Messages>

// Every catalogue loads before the first render (`prepareI18n`); only the shell's words come with
// the first script.
const loaders: Record<Locale, Loader> = {
  en: () => import("@/i18n/en").then((m) => m.en),
  "zh-TW": () => import("@/i18n/zh-TW").then((m) => m.zhTW),
  "zh-CN": () => import("@/i18n/zh-CN").then((m) => m.zhCN),
}

/** The shell's English, all there is when no catalogue could load; other keys show as themselves. */
const fallback = core as Partial<Messages> as Messages

/** `locale` is English when the wanted catalogue could not load. */
export interface Catalogue {
  locale: Locale
  messages: Messages
}

/** A chunk that fails to load (a deploy may have replaced the hashed file) gives English, or the
 * shell's words when English fails too; the next request tries again. */
export function catalogueLoader(from: Record<Locale, Loader>) {
  const loaded = new Map<Locale, Messages>()
  let latest = 0
  const fetch = async (locale: Locale): Promise<Messages | undefined> => {
    const messages = loaded.get(locale)
    if (messages) return messages
    try {
      const fetched = await from[locale]()
      loaded.set(locale, fetched)
      return fetched
    } catch {
      return undefined
    }
  }
  const load = async (locale: Locale): Promise<Catalogue> => {
    const messages = await fetch(locale)
    if (messages) return { locale, messages }
    return { locale: "en", messages: (locale === "en" ? undefined : await fetch("en")) ?? fallback }
  }
  return {
    cached: (locale: Locale): Catalogue | undefined => {
      const messages = loaded.get(locale)
      return messages ? { locale, messages } : undefined
    },
    load,
    /** `undefined` when a later choice came first. */
    choose: async (locale: Locale): Promise<Catalogue | undefined> => {
      const request = ++latest
      const catalogue = await load(locale)
      return request === latest ? catalogue : undefined
    },
  }
}

const catalogues = catalogueLoader(loaders)

/** Never rejects. */
export function prepareI18n(locale: Locale = initialLocale()): Promise<Catalogue> {
  return catalogues.load(locale)
}

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
    return z.enum(locales).safeParse(window.localStorage.getItem(storageKey)).data
  } catch {
    return undefined
  }
}

function saveLocale(locale: Locale) {
  try {
    window.localStorage.setItem(storageKey, locale)
  } catch {
    // Storage may be unavailable (private mode, blocked site data); the choice lasts this visit.
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

export function translator(messages: Messages): Translate {
  return (key, ...args) => {
    const message = messages[key] as string | ((...a: unknown[]) => string) | undefined
    if (message === undefined) return key
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
  // Outside a provider (unit tests), English once it has loaded.
  t: (key, ...args) => translator(catalogues.cached("en")?.messages ?? fallback)(key, ...args),
})

export function I18nProvider({ children }: { children: ReactNode }) {
  const [catalogue, setCatalogue] = useState<Catalogue>(
    () =>
      catalogues.cached(initialLocale()) ??
      catalogues.cached("en") ?? { locale: "en", messages: fallback },
  )

  useEffect(() => {
    document.documentElement.lang = catalogue.locale
  }, [catalogue.locale])

  const setLocale = (next: Locale) => {
    saveLocale(next)
    void catalogues.choose(next).then((chosen) => {
      if (chosen) setCatalogue(chosen)
    })
  }

  return (
    <I18nContext value={{ locale: catalogue.locale, setLocale, t: translator(catalogue.messages) }}>
      {children}
    </I18nContext>
  )
}

export function useI18n(): I18n {
  return use(I18nContext)
}

export function useT(): Translate {
  return use(I18nContext).t
}

export interface Sentence {
  key: string
  params?: Record<string, string>
}

type SentenceKey = Extract<Key, `sentence.${string}`>

/** Undefined for a key this build does not know: the translator gives such a key back. */
export function writeSentence(t: Translate, { key, params }: Sentence): string | undefined {
  const full = `sentence.${key}`
  // Every sentence takes the parameters, or nothing; a key known only at run time cannot pick one.
  const write = t as (key: SentenceKey, params: Record<string, string>) => string
  const text = write(full as SentenceKey, params ?? {})
  return text === full ? undefined : text
}

/** Falls back to the key itself when this build does not know it. */
export function useSentence(): (sentence: Sentence) => string {
  const t = useT()
  return (sentence) => writeSentence(t, sentence) ?? sentence.key
}
