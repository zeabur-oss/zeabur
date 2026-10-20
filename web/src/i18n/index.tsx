// A small typed i18n: one message object per locale, typed against English, so a missing key is a
// compile error. The locale comes from a saved preference, else the browser's languages.
//
// English is in the main bundle: it types the others and is the fallback. The other catalogues are
// chunks loaded when chosen or detected; `prepareI18n` loads the first one before the app renders,
// so a person reading Chinese never sees English first. That delays the first paint by one small
// same-origin request for them, and by nothing for English.
import { createContext, use, useEffect, useState, type ReactNode } from "react"

import { en, type Messages } from "@/i18n/en"

export const locales = ["en", "zh-TW", "zh-CN"] as const
export type Locale = (typeof locales)[number]

/** Each locale's name in its own language, for the switch. */
export const localeNames: Record<Locale, string> = {
  en: "English",
  "zh-TW": "繁體中文（台灣）",
  "zh-CN": "简体中文",
}

type Loader = () => Promise<Messages>

const loaders: Record<Exclude<Locale, "en">, Loader> = {
  "zh-TW": () => import("@/i18n/zh-TW").then((m) => m.zhTW),
  "zh-CN": () => import("@/i18n/zh-CN").then((m) => m.zhCN),
}

/** A catalogue as loaded: the locale it is in, English when the wanted one could not load. */
export interface Catalogue {
  locale: Locale
  messages: Messages
}

/**
 * Loads catalogues through `from`, keeping each one that loaded. A chunk that fails (a network
 * error, or a deploy that replaced the hashed file) gives English; the next request tries again.
 */
export function catalogueLoader(from: Record<Exclude<Locale, "en">, Loader>) {
  const loaded = new Map<Locale, Messages>([["en", en]])
  let latest = 0
  const load = async (locale: Locale): Promise<Catalogue> => {
    const messages = loaded.get(locale)
    if (messages) return { locale, messages }
    if (locale === "en") return { locale: "en", messages: en }
    try {
      const fetched = await from[locale]()
      loaded.set(locale, fetched)
      return { locale, messages: fetched }
    } catch {
      return { locale: "en", messages: en }
    }
  }
  return {
    /** The catalogue already loaded for `locale`, if any. */
    cached: (locale: Locale): Catalogue | undefined => {
      const messages = loaded.get(locale)
      return messages ? { locale, messages } : undefined
    },
    load,
    /** The person's choice: its catalogue, or `undefined` when a later choice came first. */
    choose: async (locale: Locale): Promise<Catalogue | undefined> => {
      const request = ++latest
      const catalogue = await load(locale)
      return request === latest ? catalogue : undefined
    },
  }
}

const catalogues = catalogueLoader(loaders)

/** Loads the catalogue of the starting locale; the app renders once it resolves. Never rejects. */
export function prepareI18n(): Promise<Catalogue> {
  return catalogues.load(initialLocale())
}

const storageKey = "zeabur.locale"

function isLocale(value: unknown): value is Locale {
  return typeof value === "string" && (locales as readonly string[]).includes(value)
}

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
    const value = window.localStorage.getItem(storageKey)
    return isLocale(value) ? value : undefined
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

/** `t` over one catalogue. */
export function translator(messages: Messages): Translate {
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
  const [catalogue, setCatalogue] = useState<Catalogue>(
    () => catalogues.cached(initialLocale()) ?? { locale: "en", messages: en },
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

/** A sentence the server names by key (a diagnosis, a gap, a problem), with its values. */
export interface Sentence {
  key: string
  params?: Record<string, string>
}

type SentenceKey = Extract<Key, `sentence.${string}`>

const isSentenceKey = (key: string): key is SentenceKey => Object.hasOwn(en, key)

/** A server sentence written by `t`, or undefined for a key this build does not know. Every
 * catalogue has every key English has, so a known key is always in the reader's language. */
export function writeSentence(t: Translate, { key, params }: Sentence): string | undefined {
  const full = `sentence.${key}`
  // Every sentence takes the parameters, or nothing; a key known only at run time cannot pick one.
  const write = t as (key: SentenceKey, params: Record<string, string>) => string
  return isSentenceKey(full) ? write(full, params ?? {}) : undefined
}

/** Writes a server sentence in the current locale; the key itself when this build does not know
 * it. */
export function useSentence(): (sentence: Sentence) => string {
  const t = useT()
  return (sentence) => writeSentence(t, sentence) ?? sentence.key
}
