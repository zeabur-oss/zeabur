// While "Retry as admin" waits for the passkey confirmation in its own window, and when it ends
// without admin rights: one small dialog with the state in one sentence. A granted elevation
// closes it at once and what was refused is asked again (auth/elevation.ts).
import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog"
import { Spinner } from "@/components/ui/spinner"
import { elevation, type ElevationState, type Outcome } from "@/auth/elevation"
import { goToSignIn } from "@/auth/session"
import { useT, type Translate } from "@/i18n"

function message(t: Translate, outcome: Outcome): string {
  switch (outcome.result) {
    case "not-allowed":
      return t("elevate.notAllowed")
    case "blocked":
      return t("elevate.blocked")
    case "signed-out":
      return t("elevate.signedOut")
    case "done":
    case "dismissed":
    case "cancelled":
    case "failed":
      return t("elevate.failed")
  }
}

export function ElevationDialog({ state }: { state: Exclude<ElevationState, { phase: "idle" }> }) {
  const t = useT()
  const close = () =>
    state.phase === "waiting" ? state.cancel() : elevation.set({ phase: "idle" })
  // Asking again helps unless the client may not ask at all, or the session is over.
  const again =
    state.phase === "ended" &&
    state.outcome.result !== "not-allowed" &&
    state.outcome.result !== "signed-out"
  return (
    <Dialog open onOpenChange={(open) => (open ? undefined : close())}>
      <DialogContent variant="compact">
        <div className="flex flex-col gap-3 px-5 py-4">
          <DialogTitle>{t("elevate.title")}</DialogTitle>
          {state.phase === "waiting" ? (
            <p role="status" className="flex items-center gap-2 text-sm">
              <Spinner aria-hidden="true" />
              {t("elevate.waiting")}
            </p>
          ) : (
            <p role="alert" className="text-sm">
              {message(t, state.outcome)}
            </p>
          )}
        </div>
        <div className="flex items-center justify-end gap-2 border-t px-5 py-3">
          <Button variant="ghost" size="sm" onClick={close}>
            {state.phase === "waiting" ? t("plan.cancel") : t("common.close")}
          </Button>
          {state.phase === "ended" && state.outcome.result === "signed-out" ? (
            <Button size="sm" onClick={goToSignIn}>
              {t("elevate.signIn")}
            </Button>
          ) : null}
          {state.phase === "ended" && again ? (
            <Button size="sm" onClick={state.again}>
              {t("common.retry")}
            </Button>
          ) : null}
        </div>
      </DialogContent>
    </Dialog>
  )
}
