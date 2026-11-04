// navigator, in every engine. The user's preferred locales, read from the
// platform on each access. Solidrt semantics on the standard shape: no
// `languagechange` event yet, so an app that cares reads the value again
// when it comes back to the foreground. A GUI runtime adds
// `navigator.clipboard` (clipboard.d.ts).

interface Navigator {
  /**
   * The user's preferred locales, most preferred first, as BCP 47 tags
   * (`["de-DE", "en-US"]`). Never empty: `["en-US"]` when the platform
   * reports none.
   */
  readonly languages: readonly string[]
  /** The most preferred locale: `languages[0]`. */
  readonly language: string
}

declare var navigator: Navigator
