# Docs diagrams

A docs diagram is HTML, written in the page with the site's diagram components. The text stays in
the page's own locale, and the diagram reflows: side by side on a wide screen, stacked on a phone.
The site keeps the colours, strokes and fonts in one stylesheet, so a restyle is one change in the
website repository, not in the docs.

| Component | Props | Use |
| --- | --- | --- |
| `Figure` | `label` | The whole diagram. `label` states the idea for screen readers. |
| `FigFlow` | | Its children in order, with `FigArrow` between them. |
| `FigGroup` | `label`, `row` | A box around cards, such as a cluster. `row` puts the cards side by side. |
| `FigCard` | `title`, `note`, `tag`, `hot`, `accent` | One thing. `tag` is a short status; `hot` colours it. |
| `FigArrow` | `label`, `accent` | The step from one child of `FigFlow` to the next. |

```mdx
<Figure label="Members sign in with Zeabur Identity, and Zeabur OSS shows the projects you gave them.">
<FigFlow>
<FigCard title="Member" note="A passkey on their phone or computer" />
<FigArrow label="Signs in" />
<FigCard title="Zeabur Identity" note="Checks who they are" />
<FigArrow />
<FigCard accent title="Zeabur OSS" note="Shows what you allowed" />
</FigFlow>
</Figure>
```

Use `accent` for the one thing that matters, and keep a diagram to about seven boxes. GitHub shows
the component tags as text; the site renders them.
