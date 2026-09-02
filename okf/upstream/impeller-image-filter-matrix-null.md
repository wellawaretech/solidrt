---
title: ImpellerImageFilterCreateMatrixNew returns null
description: The prebuilt Impeller the impellers 0.4.2 crate links returns null from ImpellerImageFilterCreateMatrixNew for any input (even the identity matrix), which the crate's non-null assert turns into a process abort.
created: 2026-09-02
status: unfiled
project: impellers (prebuilt Impeller via impellers 0.4.2)
versions: impellers 0.4.2
link:
---

# ImpellerImageFilterCreateMatrixNew returns null

`ImageFilter::new_matrix(&Matrix::identity(), TextureSampling::NearestNeighbor)`
panics inside impellers 0.4.2 (`assertion failed: !result.is_null()`,
lib.rs:2090): the underlying `ImpellerImageFilterCreateMatrixNew` returns
null. Reproduced 2026-09-02 on desktop Linux with a plain identity matrix,
off any GPU context (construction only), while `ImageFilter::new_blur` from
the same call site constructs fine - so it is the matrix constructor
specifically, not filter creation in general.

Two hazards for us:

- Any code path reaching `new_matrix` aborts the process (the crate asserts
  rather than returning a Result), so the constructor is effectively
  unusable until fixed upstream.
- We wanted it as the identity backdrop-capture filter for color-only
  `backdropFilter` (the backdrop argument is what makes save_layer capture
  the pixels beneath). Workaround in
  alloy/src/rendertree/kinds/filter.rs `to_backdrop_image_filter`: a
  sub-pixel blur (sigma 0.001) stands in for the identity - visually
  indistinguishable, constructible, and it still triggers the capture.
