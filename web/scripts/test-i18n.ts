// Loaded before the unit tests (bunfig.toml): English, as main.tsx loads it before the first render,
// for components rendered outside a provider. Imported by a computed path: tsconfig.node.json does
// not resolve `@/`.
import { resolve } from "node:path"

type I18n = { prepareI18n: (locale: "en") => Promise<{ locale: string }> }

const { prepareI18n } = (await import(resolve(import.meta.dirname, "../src/i18n"))) as I18n
const { locale } = await prepareI18n("en")
if (locale !== "en") throw new Error(`tests expect English, not ${locale}`)
