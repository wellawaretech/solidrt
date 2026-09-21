---
title: An unsized Button fills its row instead of sizing to its content
description: Button falls back to width 100% when size is omitted while its docs promise content sizing, so in a row it squeezes its sibling labels until they wrap; done means code and docs agree, on content sizing unless a consumer shows why not.
created: 2026-09-21
completed: 2026-09-21
---

# An unsized Button fills its row instead of sizing to its content

## Symptom

A `<Button>` without `size` in a row header next to text labels takes all
the free space; the labels shrink until they wrap (a short count label
broke onto two lines). Workaround: `size="md"`. Reported in
[[quartz-heron]] item 1.

## Cause

packages/components/src/button.tsx:

```tsx
{...(props.size ? { minWidth: SIZE_WIDTH[props.size] } : { width: "100%" })}
```

packages/components/docs/button.md says "omitted, the button sizes to its
content".

## Done looks like

The unsized button sizes to its content, as documented and as every peer's
button does; a full-width button is the caller's `layout={{ width: "100%" }}`
or a stretching parent. Before flipping it, find the in-repo callers that
rely on the fill (the components gallery, apps/console, examples, the
scaffold) and give them an explicit width.
