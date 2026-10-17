// A project's glyph, from the built-in set its namespace's icon annotation names; a folder when
// it names none.
import BookOpenIcon from "~icons/ph/book-open"
import BriefcaseIcon from "~icons/ph/briefcase"
import ChartBarIcon from "~icons/ph/chart-bar"
import ChatsIcon from "~icons/ph/chats"
import CloudIcon from "~icons/ph/cloud"
import CodeIcon from "~icons/ph/code"
import CubeIcon from "~icons/ph/cube"
import DatabaseIcon from "~icons/ph/database"
import FlaskIcon from "~icons/ph/flask"
import FolderSimpleIcon from "~icons/ph/folder-simple"
import GameControllerIcon from "~icons/ph/game-controller"
import GlobeIcon from "~icons/ph/globe"
import HeartIcon from "~icons/ph/heart"
import RocketIcon from "~icons/ph/rocket"
import StorefrontIcon from "~icons/ph/storefront"
import TerminalWindowIcon from "~icons/ph/terminal-window"

import type { Icon } from "@/lib/icon"
import { cn } from "@/lib/utils"
import type { ProjectIconName } from "@/model/project"

/** Every icon of the set: typed as a record, so an icon the API adds is a compile error here. */
export const projectGlyphs: Record<ProjectIconName, Icon> = {
  folder: FolderSimpleIcon,
  globe: GlobeIcon,
  storefront: StorefrontIcon,
  rocket: RocketIcon,
  cube: CubeIcon,
  database: DatabaseIcon,
  cloud: CloudIcon,
  code: CodeIcon,
  terminal: TerminalWindowIcon,
  chart: ChartBarIcon,
  chat: ChatsIcon,
  book: BookOpenIcon,
  flask: FlaskIcon,
  game: GameControllerIcon,
  heart: HeartIcon,
  briefcase: BriefcaseIcon,
}

/** The bare glyph in the muted colour, like a service's. */
export function ProjectIcon({
  icon,
  className,
}: {
  icon?: ProjectIconName | null
  className?: string
}) {
  const Glyph = projectGlyphs[icon ?? "folder"]
  return (
    <Glyph aria-hidden="true" className={cn("size-4 shrink-0 text-muted-foreground", className)} />
  )
}
