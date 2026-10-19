// YAML on screen: a read-only block with line numbers and a copy button, and a unified diff of two
// YAML texts. Keys are muted and values plain, so a manifest reads at a glance without an editor.
import { CopyButton } from "@/components/bits"
import { useScrollOnce } from "@/lib/in-view"
import { useT } from "@/i18n"
import { cn } from "@/lib/utils"
import { diffLines, hunks } from "@/lib/diff"

const keyLine = /^(\s*(?:- )?)([^\s:#][^:#]*?)(:)(\s.*|$)/

/** One line of YAML with its key muted. */
function Line({ text }: { text: string }) {
  if (/^\s*#/.test(text)) return <span className="text-muted-foreground">{text}</span>
  const match = text.match(keyLine)
  if (!match) return <span>{text}</span>
  const [, lead, name, colon, rest] = match
  return (
    <>
      {lead}
      <span className="text-muted-foreground">{name}</span>
      <span className="text-muted-foreground">{colon}</span>
      {rest}
    </>
  )
}

/** A YAML text with line numbers; `highlight` marks the lines of one map key (`greeting`). */
export function YamlBlock({
  yaml,
  highlight,
  label,
  className,
}: {
  yaml: string
  highlight?: string
  label: string
  className?: string
}) {
  const scrollTo = useScrollOnce<HTMLSpanElement>()
  const rows = yaml.replace(/\n$/, "").split("\n")
  const marked = highlight ? new RegExp(`^\\s*${escape(highlight)}:`) : undefined
  return (
    <figure
      aria-label={label}
      className={cn("relative min-w-0 rounded-lg border bg-muted/30", className)}
    >
      <CopyButton value={yaml} className="absolute top-2 right-2 z-10" />
      <pre className="overflow-x-auto py-3 font-mono text-xs leading-5" translate="no">
        <code className="flex flex-col">
          {rows.map((text, index) => {
            const hit = marked?.test(text)
            return (
              <span
                // oxlint-disable-next-line react/no-array-index-key -- lines are positional
                key={index}
                ref={hit ? scrollTo : undefined}
                data-highlight={hit ? "" : undefined}
                className={cn("flex pr-12", hit && "bg-brand-soft")}
              >
                <span
                  aria-hidden="true"
                  className="w-10 shrink-0 pr-3 text-right text-muted-foreground/60 select-none"
                >
                  {index + 1}
                </span>
                <span className="whitespace-pre">
                  <Line text={text} />
                </span>
              </span>
            )
          })}
        </code>
      </pre>
    </figure>
  )
}

/** A unified diff of two YAML texts, changed lines with three lines of context. */
export function YamlDiff({ before, after }: { before: string; after: string }) {
  const t = useT()
  const shown = hunks(diffLines(before, after))
  return (
    <pre
      className="overflow-x-auto rounded-lg border bg-muted/30 py-2 font-mono text-xs leading-5"
      translate="no"
      aria-label={t("plan.diff")}
    >
      <code className="flex flex-col">
        {shown.map((line, index) =>
          line === null ? (
            // oxlint-disable-next-line react/no-array-index-key -- lines are positional
            <span key={index} className="px-3 text-muted-foreground select-none">
              ⋯
            </span>
          ) : (
            <span
              // oxlint-disable-next-line react/no-array-index-key -- lines are positional
              key={index}
              className={cn(
                "flex pr-4 whitespace-pre",
                line.kind === "add" && "bg-ok-soft text-ok-strong",
                line.kind === "remove" && "bg-fail-soft text-fail-strong",
              )}
            >
              <span aria-hidden="true" className="w-6 shrink-0 text-center select-none">
                {line.kind === "add" ? "+" : line.kind === "remove" ? "−" : " "}
              </span>
              <span className="sr-only">
                {line.kind === "add"
                  ? t("plan.added")
                  : line.kind === "remove"
                    ? t("plan.removed")
                    : ""}
              </span>
              {line.text}
            </span>
          ),
        )}
      </code>
    </pre>
  )
}

function escape(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")
}
