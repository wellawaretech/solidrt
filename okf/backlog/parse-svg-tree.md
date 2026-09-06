---
title: parseSvg tree output
description: parseSvg flattens usvg's group tree into one draw list, losing group ids, group opacity and group transforms; add an opt-in tree output (groups with id/opacity/transform, paths in local space) backed by a `transform` matrix prop on views, keeping the flat list as the default.
created: 2026-08-28
---

# parseSvg tree output

## Symptom

`parseSvg` returns `{ width, height, draws }`: one flat list of fill/stroke
draws with every group transform baked into the path data. An app cannot
address a part of a document (`<g id="hour-hand">`), cannot animate a group
about its own origin without re-deriving that origin from path data, and
group `opacity` is silently dropped (listed as unsupported in
`forge/src/svg.rs`). The information exists: usvg keeps every `<g>`/`<use>`
as a `Group` with its id, and wraps any element carrying `opacity` or
`transform` in an anonymous group. `collect()` in `forge/src/svg.rs` throws
it away, and `abs_transform` baking makes the loss irreversible.

Two latent bugs sit in the same function and go with this change:
`visibility="hidden"` paths are emitted (usvg keeps them with
`is_visible() == false`, forge never checks) and `paint-order="stroke"` is
ignored (fill always first).

## Design

### API

    parseSvg(src, { color? })              -> SvgDocument { width, height, draws }
    parseSvg(src, { color?, tree: true })  -> SvgTree { width, height, children: SvgNode[] }

    type SvgNode =
      | { type: "group"; id?: string; opacity?: number;
          transform?: [a, b, c, d, e, f]; children: SvgNode[] }
      | { type: "path";  id?: string; draws: SvgDraw[] }

- Flat stays the default and keeps its shape, so the icon one-liner, the
  components `Icon` and the player app-icon do not change. Tree is opt-in;
  TS overloads on `tree: true` give each call site the right type.
- `SvgDraw` is unchanged and stays spread-clean (`<d-path {...draw} />`):
  unknown props are rejected by name in flux, so `id`/`type` live on the
  path node, not on the draw. A path node holds 1..2 draws in paint order.
- Optional keys are omitted, not set to undefined, matching the draw keys.
  `opacity` decodes through `opt_f32` (undefined = unset), so forwarding an
  absent key as `opacity={node.opacity}` is safe.

### Tree mode coordinates

Nothing is baked. Path `d` is in the enclosing group's local space, stroke
widths are raw (no `sqrt|det|` pre-scale; anisotropic scale now strokes
correctly), and a gradient's `transform` is the gradient's own. The group
node's `transform` is the SVG relative transform as a `matrix()` sextet
(the same key the gradient objects already use), omitted when identity.
usvg guarantees paths never carry transforms (a `<path transform>` becomes
a group), so the group node is the only carrier.

### `transform` prop on views

A sextet, not skew props, for two reasons:

- forge stays engine-free. Decomposing an affine into rotate/skew/scale
  props only round-trips if it inverts alloy's exact composition order
  (`compose_user` in `alloy/src/rendertree/kinds/view.rs`: center, rotate,
  scale, 3D, perspective, uncenter, translate). That order is an alloy
  detail forge must not know; a sextet is data.
- Authored transform and animation stay separate. Composed as the
  OUTERMOST step of the user chain (after x/y), `rotate`/`scale`/`x`/`y`/
  `origin` operate inside the authored frame: `<d-view
  transform={hand.transform} rotate={angle()}>` spins a clock hand about
  its own authored origin, no arithmetic, no extra wrapper. Decomposed
  props would force the app to add its angle to the authored one.

`transform?: [number x6]` on `TransformProps` (view and d-view). One field
plus one `then` in alloy, one decode in flux `properties/view.rs`, types,
one alloy test. Not animatable via `transition`: it is a base, not a value
to tween. Skew props remain possible later as an independent authoring
convenience.

### Consumer pattern (tree mode)

    function Nodes(props: { nodes: SvgNode[] }) {
      return <For each={props.nodes}>{(n) => n.type === "group"
        ? <d-view transform={n.transform} opacity={n.opacity}><Nodes nodes={n.children} /></d-view>
        : <For each={n.draws}>{(draw) => <d-path {...draw} />}</For>
      }</For>
    }

No shared render helper in the first stage: the recursion is six lines and
the core example exists to show it. Revisit if copies accumulate.

### Rust

forge keeps one shared `convert_path(path, bake: &Transform, alpha)` and
gets two small walks, `parse` (flat, bakes `abs_transform`) and
`parse_tree` (identity, emits groups). The flux plugin dispatches on the
JS option. Group `blendMode`, `isolate`, clip paths, masks and filters stay
unsupported in both modes, documented as before.

## Open

- Flat mode and group opacity: keep strictly as is (dropped), or multiply
  it into draw alpha on the way down (approximate for overlapping children,
  closer than dropping; tree mode is exact either way). Decide at pickup.
- Group `bounds: [x, y, w, h]` (usvg `abs_bounding_box`) on tree nodes so
  an app can pivot about a group's center without reading path data.
  Cheap, but derived rather than source information; second stage if asked.

## Stage 1

- alloy `kinds/view.rs` + test; flux `properties/view.rs`; `types.d.ts`
  `TransformProps`.
- forge `svg.rs` + `tests/svg.rs` (tree walk, hidden-path skip, paint
  order, group id/opacity/transform, nested and anonymous groups, path id);
  flux `forge_plugins/svg.rs`; `packages/flux-types/modules/svg.d.ts`.
- core `svg.ts` overloads + `SvgNode`/`SvgTree` exports;
  `packages/core/examples/parse-svg.tsx` switches the interactive house to
  tree mode with groups. `Icon` and player untouched.
- Same change: the example's inline sources read `<svg designSize="...">`
  since the viewBox -> designSize rename swept them; restore `viewBox`
  (the 24x24 arrow currently parses at usvg's 100x100 default).
- Docs: `packages/core/AGENTS.md`, `agents/painting.md`,
  `docs/reference/elements.md`, `examples/README.md`; components
  `docs/icon.md` only where it names the flat shape (then
  `bun scripts/build-components-docs.ts`).
- Verify: `cargo test -p forge -p alloy`, `bunx srt check`, run the example
  under `--project --port 34899` and read the tree back.

## Related

- `done/parse-svg.md`: the flat design this extends, and the currency rule
  (vector currency = path data) it keeps.
- `done/design-size-layout-space.md`: the wrapper the tree renders into.
