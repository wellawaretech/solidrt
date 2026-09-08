---
title: The 3d agent doc is 1900 lines under four headings
description: packages/3d/AGENTS.md is excellent material presented as a wall - four top-level headings across 1910 lines, with individual APIs documented inside multi-hundred-word paragraphs, so finding one prop means grepping the source instead.
created: 2026-09-08
completed: 2026-09-08
---

# The 3d agent doc is 1900 lines under four headings

## Symptom

`packages/3d/AGENTS.md` is 1910 lines and 125 KB under exactly four
headings: `## The model`, `## Components`, `## Models`, `## Traps`. Nothing
below them is addressable, and there is no table of contents.

The reported failure is the ordinary one: to answer "how do I make the
camera auto-orbit", a reader ended up grepping `src/orbit.ts`, because
`orbitSpeed` IS documented - inside a 40-line paragraph about
`createOrbitCamera`, in a bullet that opens on a different subject. The
content was right there and the document could not deliver it.

The same reader called the `## Traps` sections the highest-value prose in
the repo, and the AGENTS.md chain (a "read before you" table mapping a
TASK to a FILE) the right index shape. So this is a navigability problem
inside one file, not a rewrite: the material is good and the top-level
structure works.

## Done looks like

- Subsection headings where a reader would look one up: per component,
  per stock material, per subsystem (shadows, environment, instancing,
  models, views). Headings are the unit of addressability; a bullet
  inside a paragraph is not one.
- A table of contents with anchors at the top.
- Paragraphs that document more than one API split so each subject starts
  where its heading does.

Check the sibling docs for the same shape while in there
(`packages/core/AGENTS.md` is 533 lines and currently fine,
`packages/2d/AGENTS.md` unmeasured). Whatever convention comes out of
this applies to all of them.

Not in scope: changing what the document says. Also not a docs-generation
item - `core-docs-generated-props.md` covers generating a props reference
from the types, which is a different file and a different failure.

## Outcome

Both `packages/3d/AGENTS.md` and `packages/2d/AGENTS.md` restructured,
wording untouched (verified by word-multiset comparison against the
previous revision). `packages/core/AGENTS.md` already had the shape.

The convention for the `packages/*/AGENTS.md` chain:

- `##` per area (the model, components, traps, ...), unchanged.
- `###` per subject a reader would look up: a component, a stock
  material, a subsystem (views, culling, shadows, instancing, models).
  `####` only where a `###` subject lists several APIs of its own (the
  materials, the instancing forms, the model loading layers).
- A paragraph documents one subject and starts where its heading does;
  the "The model" bullet walls became one `###` paragraph per bullet.
- Traps stay bullets, grouped under `###` by subject. Grouping moved
  bullets; it did not change them.
- A `Contents:` list of `##` and `###` anchors under the intro once the
  file is past a few hundred lines.
