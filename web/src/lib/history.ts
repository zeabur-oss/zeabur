// The current entry of the browser history. A search box writes what is typed into the URL with
// `replace`, which keeps the entry's position; every other move (a link, Back, Forward) changes it.
// A component keyed by the position starts again from the URL after such a move, and is left alone
// while the person types. Read the starting values from the same `location` (as
// `useLocationText` does), not from a route's `useSearch`: that catches up only once the route has
// loaded, so a component starting on a move would read the previous entry's values.
import { useRouterState } from "@tanstack/react-router"

/** The current entry's position in the history. */
export function useHistoryIndex(): number {
  return useRouterState({ select: (state) => state.location.state.__TSR_index })
}

/** A text read from the current entry's search parameters, before any route validates them. */
export function useLocationText(read: (search: Record<string, unknown>) => string): string {
  return useRouterState({
    select: (state) => read(state.location.search as Record<string, unknown>),
  })
}
