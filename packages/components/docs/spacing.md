# Spacing

`space(token)` is density-scaled spacing: a `theme.spacing` token (`sm`/`md`/`lg`/`xl`) multiplied by the density scale of the nearest `<Density>` region (falling back to the global policy) and rounded to whole pixels. Use it for gaps and paddings that should tighten under `compact`/`dense` density; read `theme.spacing` directly only for distances that must not move with density. Reactive when called inside a tracked scope.

```jsx
<View layout={{ padding: space("md"), gap: space("sm") }} />
```
