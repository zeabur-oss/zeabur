// The evidence bundle names each sentence by its key and values (`` `key` (a=1, b=2) ``), as the
// server has no words of its own; this writes them in the reader's language.
import type { Sentence } from "@/i18n"

const reference = /`([a-z][\w-]*(?:\.[\w-]+)+)`(?: \(((?:[\w-]+=[^,)]*)(?:, [\w-]+=[^,)]*)*)\))?/g

function paramsOf(text: string | undefined): Record<string, string> {
  if (!text) return {}
  const params: Record<string, string> = {}
  for (const part of text.split(/, (?=[\w-]+=)/)) {
    const at = part.indexOf("=")
    params[part.slice(0, at)] = part.slice(at + 1)
  }
  return params
}

/** A key this build cannot write stays as the server wrote it. */
export function writeBundle(
  markdown: string,
  write: (sentence: Sentence) => string | undefined,
): string {
  return markdown.replace(
    reference,
    (whole, key: string, params?: string) => write({ key, params: paramsOf(params) }) ?? whole,
  )
}
