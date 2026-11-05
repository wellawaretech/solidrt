// The running application's own surface (lattice), present in every build.
declare module "sol:app" {
  /**
   * End the current app now, without running the quit hooks: back to the
   * player in a dev client, quit when standalone or at the player root (on
   * Android the activity finishes). Prefer the @solidrt/core `exit`, which
   * runs `onQuit` first.
   */
  export function exit(): void
  /**
   * Leave the current app without ending it: back to the player in a dev
   * client; otherwise to the OS background (Android moveTaskToBack, a
   * minimized window on desktop). Core's default action of an unprevented
   * `back` on Android; exposed there as `background`.
   */
  export function background(): void
  /**
   * The app's cadence-hold policy: "off" (every frame as soon as it is
   * ready), "auto" (the frame interval held to a whole number of refreshes
   * when the app cannot make the refresh rate), a whole number of refreshes
   * (at least 1) to hold it to, or "default" (the choice back to the
   * runtime, which holds on touch and remote clients and not with a mouse).
   * Applies at once and starts unheld; it survives input device changes and
   * ends with the app. The environment override (`SOLIDRT_CADENCE_HOLD`)
   * goes over it. Anything else throws. The applied state comes back on the
   * sticky "cadenceHold" bus event: `{ policy: "off" | "auto" | number,
   * hold: number }`, the hold in refreshes per frame (1 = none). Prefer the
   * @solidrt/core `setCadenceHold`.
   */
  export function setCadenceHold(policy: "off" | "auto" | "default" | number): void
  /** End the hold the runtime learned, at once, keeping the policy. */
  export function resetCadenceHold(): void
}
