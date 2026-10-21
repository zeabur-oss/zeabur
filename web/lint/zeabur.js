// The design system's `zeabur/no-unguarded-motion` (its lint/zeabur.js), which its package does not ship.

const MOVING =
  /^(?:transition(?:-(?:all|transform|translate|scale|rotate|width|height|caret|popup|sheet|listbox))?|animate-(?:spin|bounce|ping))$/

function split(token) {
  let depth = 0
  let start = 0
  for (let i = 0; i < token.length; i++) {
    const c = token[i]
    if (c === "[" || c === "(") depth++
    else if (c === "]" || c === ")") depth--
    else if (c === ":" && depth === 0) start = i + 1
  }
  return {
    variants: token.slice(0, start).split(":"),
    utility: token.slice(start).replace(/^!|!$/g, ""),
  }
}

function unguardedMotionClasses(text) {
  return text.split(/\s+/).filter((token) => {
    if (!token) return false
    const { variants, utility } = split(token)
    return (
      MOVING.test(utility) &&
      !variants.includes("motion-safe") &&
      !variants.includes("motion-reduce")
    )
  })
}

const noUnguardedMotion = {
  meta: {
    type: "problem",
    docs: {
      description:
        "Guard transitions that move or resize with motion-safe:, or use a motion utility.",
    },
  },
  create(context) {
    const check = (node, text) => {
      for (const token of unguardedMotionClasses(text))
        context.report({
          node,
          message: `"${token}" moves things for people who asked for reduced motion. Prefix it with motion-safe:, or use a motion utility (popup-motion, collapse-motion, swap-in …; the design system's README, "Motion").`,
        })
    }
    return {
      Literal(node) {
        if (typeof node.value === "string") check(node, node.value)
      },
      TemplateElement(node) {
        check(node, node.value.cooked ?? node.value.raw)
      },
    }
  },
}

export default {
  meta: { name: "zeabur" },
  rules: { "no-unguarded-motion": noUnguardedMotion },
}
