// A service's automatic image updates through Keel: which versions and when. The server says which
// policies the image's tags allow; the window is stored in UTC and picked in the browser's zone.
import type { Schemas } from "@/api/client"
import type { Translate } from "@/i18n"

export type ImagePolicy = Schemas.ImagePolicy
export type UpdateWindow = Schemas.UpdateWindow
export type When = "hourly" | "daily" | "weekly"

/** Days as `Date.getDay` numbers them, Sunday 0. */
const weekdays: readonly Schemas.Weekday[] = [
  "sunday",
  "monday",
  "tuesday",
  "wednesday",
  "thursday",
  "friday",
  "saturday",
]

/** What the form holds: the day and hour are the browser's. `other` is a setting written outside Zeabur. */
export interface UpdateForm {
  policy: ImagePolicy | "off" | "other"
  when: When
  day: number
  hour: number
}

const DEFAULT_DAY = 0
const DEFAULT_HOUR = 3

/**
 * The local minute at which a UTC hour starts on `at`'s date: 0 in most zones, 30 in India, 45 in
 * Nepal. The form's hour `h` means local `h:minute`, so it always lands on a whole UTC hour.
 */
export function localMinute(at: Date): number {
  return ((-at.getTimezoneOffset() % 60) + 60) % 60
}

/** The UTC window of a local day and hour, in the week of `now`. */
export function toUtc(form: UpdateForm, now: Date): UpdateWindow | undefined {
  if (form.when === "hourly") return undefined
  const at = new Date(now)
  at.setDate(at.getDate() + form.day - at.getDay())
  at.setHours(form.hour, localMinute(at), 0, 0)
  return {
    day: form.when === "weekly" ? weekdays[at.getUTCDay()] : undefined,
    hour: at.getUTCHours(),
  }
}

/** The local `when`, day and hour of a UTC window, in the week of `now`. */
function toLocal(
  window: UpdateWindow | null | undefined,
  now: Date,
): Pick<UpdateForm, "when" | "day" | "hour"> {
  if (!window) return { when: "hourly", day: DEFAULT_DAY, hour: DEFAULT_HOUR }
  const at = new Date(now)
  const day = window.day ? weekdays.indexOf(window.day) : at.getUTCDay()
  at.setUTCHours(window.hour, 0, 0, 0)
  at.setUTCDate(at.getUTCDate() + day - at.getUTCDay())
  return {
    when: window.day ? "weekly" : "daily",
    day: at.getDay(),
    hour: at.getHours(),
  }
}

/**
 * The Keel policy of a setting the form does not write and keeps until another choice replaces it:
 * one set outside Zeabur, or new builds on a service no longer built from source.
 */
export function keptPolicy(updates: Schemas.ImageUpdates | null | undefined): string | undefined {
  const setting = updates?.setting
  if (setting?.kind === "other") return setting.policy
  return setting?.kind === "newBuilds" ? "force" : undefined
}

/** The form for what the service does now. */
export function formOf(updates: Schemas.ImageUpdates | null | undefined, now: Date): UpdateForm {
  const setting = updates?.setting
  if (setting?.kind !== "image") {
    return {
      policy: keptPolicy(updates) ? "other" : "off",
      when: "hourly",
      day: DEFAULT_DAY,
      hour: DEFAULT_HOUR,
    }
  }
  return { policy: setting.policy, ...toLocal(setting.window, now) }
}

/** The fields of a form's time that matter for its `when`. */
function timeKey({ when, day, hour }: Pick<UpdateForm, "when" | "day" | "hour">): string {
  if (when === "hourly") return when
  return when === "daily" ? `${when} ${hour}` : `${when} ${day} ${hour}`
}

/**
 * The UTC window `form` saves: the stored one while the form still shows the time it read, so a
 * daylight-saving change inside the reference week does not move it.
 */
function windowOf(
  updates: Schemas.ImageUpdates | null | undefined,
  form: UpdateForm,
  now: Date,
): UpdateWindow | undefined {
  const setting = updates?.setting
  if (setting?.kind !== "image") return toUtc(form, now)
  const kept = timeKey(toLocal(setting.window, now)) === timeKey(form)
  return kept ? (setting.window ?? undefined) : toUtc(form, now)
}

/** The `update` of the `autoUpdate` change for `form`, none while it keeps a setting written outside Zeabur. */
export function updateOf(
  updates: Schemas.ImageUpdates | null | undefined,
  form: UpdateForm,
  now: Date,
): Schemas.Update | undefined {
  if (form.policy === "other") return undefined
  if (form.policy === "off") return { kind: "off" }
  const window = windowOf(updates, form, now)
  return window
    ? { kind: "image", policy: form.policy, window }
    : { kind: "image", policy: form.policy }
}

/**
 * The policies the form offers besides off: the allowed ones, and the saved one when the image no
 * longer allows it, so the form shows it and can still turn it off.
 */
export function policyChoices(updates: Schemas.ImageUpdates): ImagePolicy[] {
  const saved = updates.setting?.kind === "image" ? updates.setting.policy : undefined
  return saved && !updates.allowed.includes(saved) ? [...updates.allowed, saved] : updates.allowed
}

/** The image's tag, `latest` when it names none. */
export function tagOf(image: string): string {
  const last = image.split("@")[0]?.split("/").pop() ?? ""
  return last.includes(":") ? last.slice(last.indexOf(":") + 1) : "latest"
}

/** The tags a version policy moves an image tagged `tag` to, in words. Keel reads `8` as `8.0.0`. */
export function reachText(
  t: Translate,
  tag: string,
  policy: Exclude<ImagePolicy, "sameTag">,
): string {
  if (tag === "latest") return t(`imageUpdates.reach.latest.${policy}`)
  const parts = tag.replace(/^v/, "").split(".").map(Number)
  const [major = 0, minor = 0] = parts
  const reach =
    policy === "patch"
      ? t("imageUpdates.reach.within", tag, `${major}.${minor}.x`, `${major}.${minor + 1}`)
      : policy === "minor"
        ? t("imageUpdates.reach.within", tag, `${major}.x`, `${major + 1}.0`)
        : t("imageUpdates.reach.major", tag)
  return parts.length < 3 ? t("imageUpdates.reach.fullTag", reach, tag) : reach
}

/** The hint under the policy select for what the form shows. */
export function policyHint(
  t: Translate,
  updates: Schemas.ImageUpdates,
  form: UpdateForm,
  tag: string,
): string {
  const { setting } = updates
  if (form.policy === "other")
    return t("imageUpdates.other", setting?.kind === "other" ? setting.policy : "")
  if (form.policy === "off" && !updates.allowed.includes("patch"))
    return t("imageUpdates.tagOnly", tag)
  if (form.policy === "off" || form.policy === "sameTag")
    return t(`imageUpdates.policyHint.${form.policy}`)
  return reachText(t, tag, form.policy)
}

/** A window in words, such as "Sundays at 03:00 UTC". */
export function windowText(t: Translate, window: UpdateWindow): string {
  const time = `${String(window.hour).padStart(2, "0")}:00`
  return window.day
    ? t("imageUpdates.window.weekly", weekdays.indexOf(window.day), time)
    : t("imageUpdates.window.daily", time)
}

/** The name of day `day` (Sunday 0) in `locale`. */
export function dayName(day: number, locale: string, timeZone?: string): string {
  // 4 January 2026 was a Sunday.
  const date = timeZone ? new Date(Date.UTC(2026, 0, 4 + day, 12)) : new Date(2026, 0, 4 + day, 12)
  return new Intl.DateTimeFormat(locale, { weekday: "long", timeZone }).format(date)
}

/** An operator object's Keel annotations, using the same settings as registry services. */
export function readKeelUpdates(
  annotations: Record<string, string>,
  allowed: ImagePolicy[],
): Schemas.ImageUpdates {
  const policy = annotations["keel.sh/policy"]
  const schedule = annotations["keel.sh/pollSchedule"] ?? ""
  if (!policy) return { allowed }
  const match = /^0 0 ([0-9]|1[0-9]|2[0-3]) \* \* ([0-6]|\*)$/.exec(schedule)
  const window: UpdateWindow | undefined = match
    ? {
        hour: Number(match[1]),
        day: match[2] === "*" ? undefined : weekdays[Number(match[2])],
      }
    : undefined
  if (
    policy === "force" &&
    (annotations["keel.sh/matchTag"] ?? annotations["keel.sh/match-tag"]) === "true" &&
    annotations["keel.sh/trigger"] === "poll" &&
    (schedule === "@hourly" || match)
  ) {
    return { allowed, setting: { kind: "image", policy: "sameTag", ...(window ? { window } : {}) } }
  }
  return { allowed, setting: { kind: "other", policy, schedule } }
}
