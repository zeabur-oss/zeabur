import type { Schemas } from "@/api/client"

/** Install when nothing is installed, manage what Zeabur may have installed; nothing for a
 * component K3s, RKE2 or Helm manages. The dialog's plan decides between installing and removing. */
export const buttonOf = (installation: Schemas.Installation | undefined) =>
  !installation
    ? "install"
    : installation.source === "zeabur" || installation.source === "other"
      ? "manage"
      : undefined
