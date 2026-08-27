---
title: A glTF with no scenes array emits every child mesh twice
description: parseGltf falls back to treating every node index as a root when the document declares no scenes, but the walk still recurses children, so any non-root mesh is emitted once with its composed world matrix and once against the identity - duplicate parts, one at the wrong transform, and bounds covering both.
created: 2026-08-27
---

# A glTF with no scenes array emits every child mesh twice

`scene` and `scenes` are both optional in glTF 2.0. When neither is present
`parseGltf` falls back to treating every node index as a root
(`packages/3d/src/gltf.ts`):

```js
let scene = gltf.scenes?.[gltf.scene ?? 0]
let roots: number[] = scene?.nodes ?? (gltf.nodes ?? []).map((_, i) => i)
for (let index of roots) walk(index, root)
```

`walk` still recurses `node.children`, so a node that is somebody's child is
visited twice: once from its parent with the composed world matrix, and once
as a root against the identity. Every non-root mesh comes out as two parts,
one of them at the wrong world transform, and the accumulated `bounds` covers
both positions.

Nothing in the tree hits this today. Exports from mainstream tools all write a
`scenes` entry, and every fixture in `packages/3d/checks/gltf-check.ts`
declares one, which is why it has gone unnoticed. The fallback is still worth
having: a hand-written or tool-fragment .gltf legitimately omits `scenes`, and
"draw everything" is the right intent. Only the implementation double-counts.

## Done

A sceneless document produces one part per mesh node, at the same world
transforms it would get if the file declared a scene naming its true roots.

## Shape

One line in the fallback: keep only the indices no other node names as a
child. The alternative - a visited set inside `walk` - fixes the same symptom
and additionally makes a malformed file whose `children` form a cycle
terminate instead of recursing until the stack goes, which the current code
does not do on any path, scenes or no scenes. That second property is the
reason to prefer it.

A fixture in `gltf-check.ts` covering a scene-less document with a nested
node, asserting the part count and the child's world position. The rig builds
its documents in memory, so this is a few lines beside the existing ones.
