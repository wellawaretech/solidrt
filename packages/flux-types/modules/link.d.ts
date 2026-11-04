// Opening a link: the platform's opener hands it to the app that handles
// it (a browser for https:, the mail app for mailto:, the store app for a
// store link). Listed in `Flux.capabilities` as "link" only where the
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
}
