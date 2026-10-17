// A YAML editor: CodeMirror 6 with the YAML language, line numbers, undo and Tab to indent. It is
// its own chunk, loaded only when a person starts editing, so reading YAML costs nothing extra.
// The view is made once per mount by a stable ref callback (React 19 runs its cleanup on
// unmount). `initial`, `label` and `onChange` are read once: pass a state setter as `onChange`.
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands"
import { yaml } from "@codemirror/lang-yaml"
import {
  bracketMatching,
  HighlightStyle,
  indentOnInput,
  syntaxHighlighting,
} from "@codemirror/language"
import { EditorState } from "@codemirror/state"
import { EditorView, highlightActiveLine, keymap, lineNumbers } from "@codemirror/view"
import { tags } from "@lezer/highlight"
import { useState } from "react"

// The same reading as the YAML view: keys and punctuation muted, values plain, comments quiet.
const highlight = HighlightStyle.define([
  {
    tag: [tags.propertyName, tags.definition(tags.propertyName)],
    color: "var(--muted-foreground)",
  },
  { tag: [tags.punctuation, tags.separator, tags.meta], color: "var(--muted-foreground)" },
  { tag: tags.comment, color: "var(--muted-foreground)", fontStyle: "italic" },
  {
    tag: [tags.string, tags.number, tags.bool, tags.null, tags.keyword],
    color: "var(--foreground)",
  },
])

// Colours come from the page's tokens, so light and dark follow the theme.
const theme = EditorView.theme({
  "&": {
    backgroundColor: "var(--card)",
    color: "var(--foreground)",
    fontSize: "0.75rem",
    borderRadius: "0.75rem",
    border: "1px solid var(--border)",
  },
  "&.cm-focused": { outline: "2px solid var(--ring)", outlineOffset: "1px" },
  ".cm-scroller": { fontFamily: "var(--font-mono)", lineHeight: "1.25rem", minHeight: "24rem" },
  ".cm-content": { caretColor: "var(--foreground)", padding: "0.75rem 0" },
  ".cm-gutters": {
    backgroundColor: "transparent",
    color: "var(--muted-foreground)",
    border: "none",
  },
  ".cm-activeLine, .cm-activeLineGutter": { backgroundColor: "var(--muted)" },
  "&.cm-focused .cm-selectionBackground, .cm-selectionBackground, ::selection": {
    backgroundColor: "var(--brand-soft)",
  },
})

export function YamlEditor({
  initial,
  label,
  onChange,
}: {
  /** The text to start from. */
  initial: string
  label: string
  /** Called with the whole text after each edit; a state setter. */
  onChange: (text: string) => void
}) {
  // One callback for the component's life: a new one each render would remake the view.
  const [mount] = useState(() => (host: HTMLDivElement | null) => {
    if (!host) return
    // CodeMirror adds its styles in a <style> element in <head>, which the page's Content
    // Security Policy allows (`style-src 'unsafe-inline'`; web/DESIGN.md, "Content Security
    // Policy").
    const view = new EditorView({
      parent: host,
      state: EditorState.create({
        doc: initial,
        extensions: [
          lineNumbers(),
          history(),
          indentOnInput(),
          bracketMatching(),
          highlightActiveLine(),
          syntaxHighlighting(highlight),
          yaml(),
          keymap.of([indentWithTab, ...defaultKeymap, ...historyKeymap]),
          EditorView.contentAttributes.of({ "aria-label": label }),
          EditorView.updateListener.of((update) => {
            if (update.docChanged) onChange(update.state.doc.toString())
          }),
          theme,
        ],
      }),
    })
    return () => view.destroy()
  })
  return <div ref={mount} translate="no" />
}
