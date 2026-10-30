// Protocol: crates/zeabur-api/src/cluster/node_shells.rs.
import type { Schemas } from "@/api/client"
import type { Health } from "@/model/service"

type NodeShell = Schemas.NodeShellView

/** Never a project's: only cluster administrators may create or exec pods in it. */
export const NODE_SHELL_NAMESPACE = "zeabur-node-shell"

export function nodeShellUrl(location: Pick<Location, "protocol" | "host">, pod: string): string {
  const scheme = location.protocol === "https:" ? "wss:" : "ws:"
  return `${scheme}//${location.host}/api/v1/cluster/node-shells/${encodeURIComponent(pod)}/terminal`
}

/** An ended shell is grey whatever ended it: the deadline is how it is meant to stop. */
export function shellHealth(shell: Pick<NodeShell, "phase" | "reason">): Health {
  if (shell.phase === "running") return "ok"
  if (shell.phase === "ended") return "idle"
  return shell.reason && shell.reason !== "ContainerCreating" ? "warn" : "progress"
}

export type ShellWord = "starting" | "stuck" | "running" | "deadline" | "ended"

export function shellWord(shell: Pick<NodeShell, "phase" | "reason">): ShellWord {
  if (shell.phase === "running") return "running"
  if (shell.phase === "ended") return shell.reason === "DeadlineExceeded" ? "deadline" : "ended"
  return shellHealth(shell) === "warn" ? "stuck" : "starting"
}

export function shellsOn(shells: readonly NodeShell[], node: string): NodeShell[] {
  return shells.filter((shell) => shell.node === node)
}

/** Only its opener may enter, and only while it runs. */
export const enters = (shell: Pick<NodeShell, "mine" | "phase">) =>
  shell.mine && shell.phase === "running"

export function subjectName(subject: Schemas.ShellSubject): string {
  return subject.namespace ? `${subject.namespace}/${subject.name}` : subject.name
}
