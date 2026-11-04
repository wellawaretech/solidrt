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
}
