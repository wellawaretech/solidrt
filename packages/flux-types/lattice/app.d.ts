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
   * Make this install the handler for the app's own scheme (its appId, as
   * in `com.example.app://settings`), so links of it open the app: on Linux
   * a desktop entry plus the mimeapps.list default, on Windows the
   * HKCU\Software\Classes protocol key, both for the current user. Throws
   * when the registration cannot be written. Android and macOS packages
   * declare the scheme themselves, so the call has nothing to do there; a
   * dev client registers nothing and logs a warning. Prefer the
   * @solidrt/core `registerProtocolHandler`, the same call documented with
   * the rest of the link surface.
   */
  export function registerProtocolHandler(): void
}
