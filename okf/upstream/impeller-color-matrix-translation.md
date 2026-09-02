---
title: ImpellerColorMatrix translation column is normalized, not 0..255
description: The impeller.h doc (and impellers 0.4.2 doc comment) says the color-matrix translation column is 0..255; the shipped implementation adds it raw in normalized 0..1 color space.
created: 2026-09-02
status: unfiled
project: impellers (prebuilt Impeller via impellers 0.4.2)
versions: impellers 0.4.2
link:
---

# ImpellerColorMatrix translation column is normalized, not 0..255

`ImpellerColorMatrix` documents (impeller.h, surfaced verbatim in the
impellers crate's sys docs): "The translation column (m[4], m[9], m[14],
m[19]) must be specified in non-normalized 8-bit unsigned integer space (0
to 255). Values outside this range will produce undefined results." Its
invert example uses 255 offsets accordingly.

Observed with the prebuilt Impeller the impellers 0.4.2 crate links
(desktop GLES, 2026-09-02, the rendertree `filter` prop): a color filter
built per the doc renders wrong, and the normalized form renders right.

- invert per the doc (slope -1, offset 255) -> solid white. Offset 1.0 ->
  correct inversion.
- contrast 2 with the doc's scaling (offset (0.5 - 0.5 * 2) * 255 = -127.5)
  -> solid black. Offset -0.5 -> correct contrast.
- Matrices with a zero translation column (grayscale, sepia, saturate,
  hue-rotate, brightness) render correctly either way, which is how the
  discrepancy stays hidden until a filter needs an offset.

So the implementation adds the column raw in normalized 0..1 color space.
Flutter's own `ColorFilter.matrix` docs make the same 0..255 claim, so this
is likely an Impeller C API implementation/documentation mismatch inherited
from there rather than an impellers packaging bug.

Our code (alloy/src/rendertree/kinds/filter.rs `color_matrix`) follows the
observed behavior and emits normalized offsets; if Impeller ever changes to
match its header, invert/contrast filters go visibly wrong and the
`color_matrix_composition` unit test's comment is the pointer back here.
