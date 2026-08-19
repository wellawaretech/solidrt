# Image

Loads and displays an image from a URL or raw bytes (`src: string | Uint8Array`). URL loads are shared runtime-wide: mounts of the same URL reuse one fetch and one texture, and the bytes are cached on disk (fetched with `cache: "force-cache"` - no freshness check, so use versioned URLs for content that changes). Concurrent asset fetches are kept polite with a per-host limit; a failed load rejects the mounts sharing it and a later remount retries.

```jsx
import { Image } from "@solidrt/components"

<Image
  src="https://example.com/avatar.png"
  fallback={PLACEHOLDER_PNG}
  layout={{ width: 64, height: 64 }}
/>
```

With `fit` the image fills whatever box `layout` gives the component - numbers, `pct()`, or flex - and the fit decides how the pixels map into it (CSS object-fit, centered; `"cover"` is the ported-web-hero-image answer). Without `fit`, only numeric layout sizes reach the image; anything else draws at intrinsic size.

```jsx
<Image src={hero} fit="cover" layout={{ width: pct(100), height: 240 }} />
```

A failing `src` is contained by the component: the `fallback` shows, or the `backgroundColor` placeholder stays; the error does not propagate to an outer `<Errored>` boundary. `onLoad` fires each time a source finishes loading, `onError` when `src` fails.
