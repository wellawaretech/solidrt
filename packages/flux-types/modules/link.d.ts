// Links: opening one, and the app's own scheme. Opening a link: the
// platform's opener hands it to the app that handles it (a browser for
// https:, the mail app for mailto:, the store app for a store link). Listed in `Flux.capabilities` as "link" only where the
// platform has an opener (a Linux session with neither the
// xdg-desktop-portal nor xdg-open has none), and a call there rejects
// with an error named "NotSupportedError". Under `sol test` no link is
// opened: the test host records it (`app.requestedLinks()`), so a test
// engine lists "link" whatever its platform has. A dev client opens it
// and records it too, for an agent to read back.

declare module "flux:link" {
  /**
   * Hands `link` to the platform's opener. Always goes out to the
   * platform: a link of the app's own scheme comes back in through
   * `onLink`, and in-app navigation is the router's `createLink(...).go()`.
   *
   * Resolves once the platform's opener accepted the link. Rejects when the
   * platform refuses it (on Android, Windows and macOS: nothing handles the
   * link), with a "NotSupportedError" where the platform has no opener.
   * Says nothing about what the other app does with it. On Linux the
   * `xdg-open` fallback runs in the background, so a link nothing handles
   * still resolves there.
   *
   * Throws a TypeError on a link without a scheme (`"/settings"`,
   * `"example.com"`): any scheme is accepted, but there must be one, at
   * least two characters long, so a Windows drive path (`"C:\\file"`) is not
   * opened as a local file.
   *
   * ```ts
   * await openLink("https://example.com/credits")
   * ```
   */
  export function openLink(link: string): Promise<void>
  /**
   * Makes this executable the handler for the app's own scheme (its appId,
   * as in `com.example.app://settings`), so links of it open the app: on
   * Linux a desktop entry plus the mimeapps.list default, on Windows the
   * HKCU\Software\Classes protocol key, both for the current user. Throws
   * when the registration cannot be written. Android and macOS packages
   * declare the scheme themselves, so the call has nothing to do there; a
   * dev client registers nothing and logs a warning; a run with no app
   * identity (headless flux, a dev run of the runtime) throws. Under
   * `sol test` nothing is written. Prefer the @solidrt/core
   * `registerProtocolHandler`, the same call documented with the rest of
   * the link surface.
   */
  export function registerProtocolHandler(): void
}
