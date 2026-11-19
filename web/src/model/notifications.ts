import type { Schemas } from "@/api/client"

type Settings = Schemas.AlertmanagerChannelSettings
export type ChannelKind = Settings["kind"]
export type Event = Schemas.AlertmanagerEvent

/** Alerting comes with the metrics component. */
export const METRICS_COMPONENT = "victoria-metrics"

/** `NAME_MAX` in zeabur-modules' alertmanager. */
export const NAME_MAX = 30

export const KINDS: ChannelKind[] = ["slack", "discord", "telegram", "email", "webhook", "ntfy"]

export type Field =
  | "webhookUrl"
  | "botToken"
  | "chatId"
  | "to"
  | "from"
  | "smarthost"
  | "username"
  | "password"
  | "url"
  | "bearerToken"
  | "server"
  | "topic"
  | "accessToken"

/** The fields of each kind, in order; the second value says whether it may stay empty. */
export const FIELDS: Record<ChannelKind, [Field, boolean][]> = {
  slack: [["webhookUrl", false]],
  discord: [["webhookUrl", false]],
  telegram: [
    ["botToken", false],
    ["chatId", false],
  ],
  email: [
    ["to", false],
    ["from", false],
    ["smarthost", false],
    ["username", true],
    ["password", true],
  ],
  webhook: [
    ["url", false],
    ["bearerToken", true],
  ],
  ntfy: [
    ["server", true],
    ["topic", false],
    ["accessToken", true],
  ],
}

/** Fields whose value goes into the Secret: typed as passwords and never shown again. */
export const SECRET_FIELDS: Field[] = [
  "webhookUrl",
  "botToken",
  "password",
  "url",
  "bearerToken",
  "accessToken",
]

/** Fields kept in the channel's Secret. An edit shows them empty, and empty keeps them. */
export const STORED_FIELDS: Field[] = [...SECRET_FIELDS, "server", "topic"]

export interface ChannelForm {
  name: string
  kind: ChannelKind
  values: Partial<Record<Field, string>>
  events: Event[]
}

export function nameProblem(name: string): boolean {
  return !/^[a-z]([a-z0-9-]*[a-z0-9])?$/.test(name) || name.length > NAME_MAX
}

/** The fields still needed, in order: required and empty, or a chat ID that is not a number. */
export function missing(form: ChannelForm): Field[] {
  return FIELDS[form.kind]
    .filter(([field, optional]) => {
      const value = form.values[field]?.trim() ?? ""
      if (field === "chatId") return !/^-?\d+$/.test(value) || Number(value) === 0
      return !optional && value === ""
    })
    .map(([field]) => field)
}

/** The fields still needed in an edit: a stored field may stay empty, but ntfy's topic is stored
 * with its server, so a new server needs the topic again. */
export function missingInEdit(form: ChannelForm): Field[] {
  const typed = (field: Field) => (form.values[field]?.trim() ?? "") !== ""
  const needed = missing(form).filter((field) => !STORED_FIELDS.includes(field))
  return typed("server") && !typed("topic") ? [...needed, "topic"] : needed
}

/** The form of a channel to edit: its shown settings, the stored ones empty. */
export function formOf(channel: Schemas.AlertmanagerChannel): ChannelForm {
  return {
    name: channel.name,
    kind: channel.kind as ChannelKind,
    values: { ...channel.fields },
    events: channel.events,
  }
}

export function settingsOf(form: ChannelForm): Settings {
  const v = (field: Field) => form.values[field]?.trim() ?? ""
  switch (form.kind) {
    case "slack":
    case "discord":
      return { kind: form.kind, webhookUrl: v("webhookUrl") }
    case "telegram":
      return { kind: "telegram", botToken: v("botToken"), chatId: Number(v("chatId")) }
    case "email":
      return {
        kind: "email",
        to: v("to"),
        from: v("from"),
        smarthost: v("smarthost"),
        username: v("username"),
        password: form.values.password ?? "",
      }
    case "webhook":
      return { kind: "webhook", url: v("url"), bearerToken: v("bearerToken") }
    case "ntfy":
      return { kind: "ntfy", server: v("server"), topic: v("topic"), accessToken: v("accessToken") }
  }
}

export const statusHealth = { active: "ok", rejected: "fail", waiting: "idle" } as const
