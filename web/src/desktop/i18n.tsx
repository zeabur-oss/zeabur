import { FluentBundle, FluentResource, type FluentVariable } from "@fluent/bundle"
import { createContext, useContext, useMemo, useState, type ReactNode } from "react"

import en from "./locales/en.ftl?raw"
import ja from "./locales/ja.ftl?raw"
import zhCN from "./locales/zh-CN.ftl?raw"
import zhTW from "./locales/zh-TW.ftl?raw"

export type Lang = "en" | "zh-TW" | "zh-CN" | "ja"

export const languages: readonly { value: Lang; label: string }[] = [
  { value: "en", label: "English" },
  { value: "zh-TW", label: "繁體中文" },
  { value: "zh-CN", label: "简体中文" },
  { value: "ja", label: "日本語" },
]

const sources: Record<Lang, string> = { en, "zh-TW": zhTW, "zh-CN": zhCN, ja }

function bundle(lang: Lang): FluentBundle {
  const b = new FluentBundle(lang)
  b.addResource(new FluentResource(sources[lang]))
  return b
}

const english = bundle("en")

export type T = (id: string, args?: Record<string, FluentVariable>) => string

function translator(lang: Lang): T {
  const own = lang === "en" ? english : bundle(lang)
  return (id, args) => {
    for (const b of [own, english]) {
      const message = b.getMessage(id)
      if (message?.value) return b.formatPattern(message.value, args)
    }
    return id
  }
}

/** The system language first, English when none of the four match. */
export function systemLang(): Lang {
  for (const tag of navigator.languages) {
    const lower = tag.toLowerCase()
    if (lower.startsWith("ja")) return "ja"
    if (lower.startsWith("zh")) {
      return /-(tw|hk|mo|hant)/.test(lower) ? "zh-TW" : "zh-CN"
    }
    if (lower.startsWith("en")) return "en"
  }
  return "en"
}

const Context = createContext<{ lang: Lang; t: T; setLang: (lang: Lang) => void } | null>(null)

export function I18n({ initial, children }: { initial: Lang; children: ReactNode }) {
  const [lang, setLang] = useState(initial)
  const value = useMemo(() => ({ lang, t: translator(lang), setLang }), [lang])
  document.documentElement.lang = lang
  return <Context value={value}>{children}</Context>
}

export function useI18n() {
  const value = useContext(Context)
  if (!value) throw new Error("useI18n outside I18n")
  return value
}
