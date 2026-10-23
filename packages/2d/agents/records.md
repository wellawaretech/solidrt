# @solidrt/2d - the records layer

The records layer: JS-owned float records per sprite, raw writes and the
JS pick walk. Read ../AGENTS.md first: it carries the model this file
builds on.

## The records layer

The records layer (`createRecordLayer`) keeps the old model whole: 20
JS-owned floats per sprite `[cx, cy, w, h, u0, v0, u1, v1, rot, tint
rgba, minScreenPx, maxScreenPx, atlas, outline rgb, outlineWidth]`
(`INSTANCE_FLOATS`; `atlas` is the frame's texture as its index in
`layer.atlases`; the outline is read by a distance-field atlas only), draw order =
insertion order (or key order with `orderBy` - see below), remove
shifts, raw writes, JS pick walk. The raw path speaks @solidrt/3d's
record-mesh verbs, one dimension down: `records(layer)` is the mirror
(read it at use time - growth replaces it), `updateRecords(layer, {
first?, count? })` publishes a RANGE of it at the flush (the whole
mirror by default; ten moved records of ten thousand cost ten - one
`writeBuffer` at the range's byte offset, clipped to the live sprites;
under `orderBy` the drawn prefix whole through the lease instead, since
the core gathers it into key order during the copy), and
`setRecordCount(layer, n)` dials the drawn prefix - the first n live
sprites draw and pick, the rest stay live but undrawn and unpicked,
and the dial stays in force as sprites come and go (write ahead, dial
after; `layer.count` is the live total regardless). The sprite verbs
mark their own record, so a `setSprite` here publishes one record, not
the prefix. The POPULATION verbs differ by kind and stay so: a record
mesh's records are opaque data (`setRecords` copies in and grows), a
record layer's are sprites (`addSprite`/`destroySprite`), so the copy-in
here is `records(layer).set(src)` then `updateRecords(layer)`. The
publish itself is the core's record stream (`createRecordStream` in
`@solidrt/core/gpu`: the GPU buffer, the byte mirror, the dirty range,
the range-or-whole publish, growth into a replacement), the one model
the 3d instance streams run on too; this file only decides what to
mark and when to flush. The node layer speaks the same two verbs over
its SPRITE records: `records(spriteLayer)` is the sprite mirror
(`SPRITE_FLOATS` floats per slot: `[u0, v0, u1, v1, tint rgba,
renderOrder, minScreenPx, maxScreenPx, atlas, outline rgb,
outlineWidth]`, slot-indexed, poses stay the core's) and
`updateRecords(spriteLayer, { first?, count? })` publishes the slot
range, so a bulk restyle (a palette cycle over thousands of sprites) is
one loop and one range write instead of a setSprite each - the 3d
`records(instancedMesh)` pairing, one dimension down. Both layer
kinds take a `stream` beyond 0 on the same verbs for the material's
style streams (`records(layer, 1)`, `updateRecords(layer, { stream: 1
})`), which grow, publish and (on a records layer) shift with the
layer's own record. It is the
escape hatch for motion only JS can compute at scale (measured 30k
sprites: 12.9ms raw records vs 30.8ms via setSprite; both figures are
the WRITE path only - whatever computes the motion is excluded and is
usually the dominant cost, e.g. a 24k-particle sim measured ~25ms with
a near-free publish) - the axis is
WHERE MOTION IS COMPUTED, not retained-vs-dynamic. The sprite functions
(addSprite/setSprite/getSprite/destroySprite) work on both layer kinds;
record sprites have `node: null` and no groups. `tests/records.test.tsx`
pins the range, the dial, growth and the ordered publish on painted
frames.
