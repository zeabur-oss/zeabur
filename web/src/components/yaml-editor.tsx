// A YAML editor: CodeMirror 6 with the YAML language, line numbers, undo and Tab to indent. It is
// its own chunk, loaded only when a person starts editing, so reading YAML costs nothing extra.
// The view is made once per mount by a stable ref callback (React 19 runs its cleanup on
// unmount). `initial`, `label` and `onChange` are read once: pass a state setter as `onChange`.
// `marks` tint lines with a problem: the line in its tone, a dot in a gutter of its own and the
// message at the end of the line; a list beside the editor says the same for screen readers.
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands"
import { yaml } from "@codemirror/lang-yaml"
import {
  bracketMatching,
  HighlightStyle,
  indentOnInput,
  syntaxHighlighting,
} from "@codemirror/language"
import {
  EditorState,
  RangeSet,
  RangeSetBuilder,
  StateEffect,
  StateField,
  type Text,
} from "@codemirror/state"
import {
  Decoration,
  type DecorationSet,
  EditorView,
  gutter,
  GutterMarker,
  highlightActiveLine,
  keymap,
  lineNumbers,
  WidgetType,
} from "@codemirror/view"
import { tags } from "@lezer/highlight"
import { useEffect, useRef, useState } from "react"

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
  ".cm-line.cm-mark-fail": { backgroundColor: "var(--fail-soft)" },
  ".cm-line.cm-mark-warn": { backgroundColor: "var(--warn-soft)" },
  ".cm-mark-note": { fontFamily: "var(--font-sans)", paddingLeft: "1.5rem" },
  ".cm-mark-note-fail": { color: "var(--fail-strong)" },
  ".cm-mark-note-warn": { color: "var(--warn-strong)" },
  ".cm-mark-gutter": { width: "0.875rem", paddingLeft: "0.375rem" },
  ".cm-mark-dot": {
    display: "inline-block",
    width: "0.5rem",
    height: "0.5rem",
    borderRadius: "9999px",
    verticalAlign: "middle",
  },
  ".cm-mark-dot-fail": { backgroundColor: "var(--fail)" },
  ".cm-mark-dot-warn": { backgroundColor: "var(--warn)" },
})

export interface LineMark {
  /** From 1. */
  line: number
  tone: "fail" | "warn"
  message: string
}

/** What a page may do to the editor once it is made. */
export interface YamlEditorHandle {
  /** Moves the cursor to `line`, scrolls it into view and focuses the editor. */
  jump: (line: number) => void
  /** Replaces the whole text, as an edit the person can undo. */
  replace: (text: string) => void
}

class Note extends WidgetType {
  readonly text: string
  readonly tone: LineMark["tone"]
  constructor(text: string, tone: LineMark["tone"]) {
    super()
    this.text = text
    this.tone = tone
  }
  override eq(other: Note) {
    return other.text === this.text && other.tone === this.tone
  }
  toDOM() {
    const span = document.createElement("span")
    span.className = `cm-mark-note cm-mark-note-${this.tone}`
    span.setAttribute("aria-hidden", "true")
    span.textContent = `← ${this.text}`
    return span
  }
}

class Dot extends GutterMarker {
  readonly tone: LineMark["tone"]
  constructor(tone: LineMark["tone"]) {
    super()
    this.tone = tone
  }
  override eq(other: Dot) {
    return other.tone === this.tone
  }
  override toDOM() {
    const span = document.createElement("span")
    span.className = `cm-mark-dot cm-mark-dot-${this.tone}`
    return span
  }
}

interface Marked {
  lines: DecorationSet
  dots: RangeSet<GutterMarker>
}

function marking(doc: Text, marks: readonly LineMark[]): Marked {
  // One mark per line: a failure over a note.
  const byLine = new Map<number, LineMark>()
  for (const mark of marks) {
    if (mark.line < 1 || mark.line > doc.lines) continue
    const held = byLine.get(mark.line)
    if (!held || (held.tone === "warn" && mark.tone === "fail")) byLine.set(mark.line, mark)
  }
  const lines = new RangeSetBuilder<Decoration>()
  const dots = new RangeSetBuilder<GutterMarker>()
  for (const mark of [...byLine.values()].toSorted((a, b) => a.line - b.line)) {
    const line = doc.line(mark.line)
    lines.add(line.from, line.from, Decoration.line({ class: `cm-mark-${mark.tone}` }))
    lines.add(
      line.to,
      line.to,
      Decoration.widget({ widget: new Note(mark.message, mark.tone), side: 1 }),
    )
    dots.add(line.from, line.from, new Dot(mark.tone))
  }
  return { lines: lines.finish(), dots: dots.finish() }
}

const setMarks = StateEffect.define<readonly LineMark[]>()

const marked = StateField.define<Marked>({
  create: () => ({ lines: Decoration.none, dots: RangeSet.empty }),
  update(value, transaction) {
    let next = {
      lines: value.lines.map(transaction.changes),
      dots: value.dots.map(transaction.changes),
    }
    for (const effect of transaction.effects)
      if (effect.is(setMarks)) next = marking(transaction.state.doc, effect.value)
    return next
  },
  provide: (field) => EditorView.decorations.from(field, (value) => value.lines),
})

export function YamlEditor({
  initial,
  label,
  onChange,
  marks,
  onReady,
}: {
  /** The text to start from. */
  initial: string
  label: string
  /** Called with the whole text after each edit; a state setter. */
  onChange: (text: string) => void
  marks?: readonly LineMark[]
  /** Receives the handle once the editor is made. */
  onReady?: (handle: YamlEditorHandle) => void
}) {
  const view = useRef<EditorView | null>(null)
  // One callback for the component's life: a new one each render would remake the view.
  const [mount] = useState(() => (host: HTMLDivElement | null) => {
    if (!host) return
    // CodeMirror adds its styles in a <style> element in <head>, which the page's Content
    // Security Policy allows (`style-src 'unsafe-inline'`; web/DESIGN.md, "Content Security
    // Policy").
    const made = new EditorView({
      parent: host,
      state: EditorState.create({
        doc: initial,
        extensions: [
          marked,
          gutter({
            class: "cm-mark-gutter",
            markers: (v) => v.state.field(marked).dots,
          }),
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
    view.current = made
    onReady?.({
      jump: (n) => {
        const doc = made.state.doc
        const line = doc.line(Math.min(Math.max(n, 1), doc.lines))
        made.dispatch({ selection: { anchor: line.from }, scrollIntoView: true })
        made.focus()
      },
      replace: (text) =>
        made.dispatch({ changes: { from: 0, to: made.state.doc.length, insert: text } }),
    })
    return () => {
      view.current = null
      made.destroy()
    }
  })
  useEffect(() => {
    view.current?.dispatch({ effects: setMarks.of(marks ?? []) })
  }, [marks])
  return <div ref={mount} translate="no" className="min-w-0" />
}
