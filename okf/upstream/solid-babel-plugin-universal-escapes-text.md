---
title: Solid babel plugin HTML-escapes text in universal mode
description: With generate universal, static string children are passed through escapeHTML ("<" becomes "&lt;", "&" becomes "&amp;", ">" untouched) and JSX text entities are never decoded, so createTextNode receives HTML-escaped strings that no parser will ever unescape; the universal renderer has no HTML downstream and must emit text verbatim.
project: "@solidjs/babel-plugin" (github.com/solidjs/solid, packages/babel-plugin)
versions: "@solidjs/babel-plugin 2.0.0-rc.3 (also rc.4 and next HEAD as of 2026-08-30)"
status: filed
link: https://github.com/solidjs/solid/issues/3127
created: 2026-08-30
---

# Solid babel plugin HTML-escapes text in universal mode

Found 2026-08-30 in external feedback on the render-lab demo (the 54
project, item 1 of its feedback file): `<d-text>{"<span> restyles a run"}</d-text>`
renders `&lt;span> restyles a run` on screen, and the render tree reports the
node's text with the `&lt;` already in it. The reporter's read was exactly
right: the escaping happens before the string reaches the node, it is
asymmetric (`<` yes, `>` no), and there is no DOM anywhere that could want it.

Nothing in our tree escapes. The transform we run
(`packages/cli/src/bundle/bundler.ts`: `[solid, { moduleName: "@solidrt/core",
generate: "universal" }]`) does it on its own:

```tsx
let A = () => <d-text>{"<span> restyles a run"}</d-text>
let B = () => <d-text>{"<b> & <i>"}</d-text>
let C = () => <text>lit &lt; text</text>
```

compiles to

```js
_$insertNode(_el$, _$createTextNode(`&lt;span> restyles a run`));
_$insertNode(_el$3, _$createTextNode(`&lt;b> &amp; &lt;i>`));
_$insertNode(_el$5, _$createTextNode(`lit &lt; text`));
```

Case C is the second half of the same bug: the JSX entity is never decoded,
so a user who writes `&lt;` the JSX way gets the six characters on screen too.
There is no way to put a `<` into static text at all.

## Draft report

Two sites in the universal renderer (function names as shipped in
`@solidjs/babel-plugin` 2.0.0-rc.3 `index.js`):

1. Universal `transformChildren` calls `transformNode(path)` with no `info`,
   so a static child expression takes the escaping branch of `transformNode`:

   ```js
   const text = staticValue !== undefined
     ? info.doNotEscape ? String(staticValue) : escapeHTML(String(staticValue))
     : trimWhitespace(node.extra?.raw ?? "");
   ```

   Text-mode `escapeHTML` replaces `&` and `<` only, which is where the
   `<`-yes-`>`-no asymmetry comes from.

2. The `JSXText` branch of the same expression emits `node.extra.raw`, the
   undecoded source text, so `&lt;` (or any entity) survives verbatim.

Both are correct for the DOM generator, where the string is spliced into an
HTML template and the parser unescapes it, and for `<script>`/`<style>`
(raw-text elements, where `doNotEscape` already selects the raw path). The
universal renderer has no parser downstream: the string goes straight into
the host's `createTextNode`, so every static string containing `<` or `&`
ends up wrong on screen, and there is no spelling that produces a literal
`<` in static text. Dynamic text (`insert(el, () => expr)`) is unaffected,
which is why this hides in apps until a docs-like string literal shows up.

Repro: transform the three lines above with
`plugins: [jsx, [solid, { moduleName: "x", generate: "universal" }]]` and
look at the `createTextNode` arguments.

Suggested fix, in the universal generator only: `transformChildren` passes
`{ doNotEscape: true }` to `transformNode`, and the `JSXText` branch uses the
decoded `node.value` (Babel decodes entities into `value` and keeps the
source in `extra.raw`) when `config.generate === "universal"`. `trimWhitespace`
applies unchanged since `value` and `raw` differ only in the entities. The
DOM and SSR generators keep their current behaviour.

## Upstream status

Filed 2026-08-30 as solidjs/solid#3127. Checked the same day before filing: not known. No issue, PR or discussion on solidjs/solid or
ryansolid/dom-expressions mentions it (searched escape, escapeHTML, entity,
universal, createTextNode, doNotEscape, custom renderer). Nearest neighbours,
all different bugs: dom-expressions #155 (universal static text dropped to an
empty string under transform-template-literals, 2022, fixed), solid #1088 and
#691 (DOM-mode entity handling, closed by design because the DOM template
parser decodes them - the very step universal lacks).

The source on `next` (packages/babel-plugin/src, last touched 2026-08-30) has
both sites unchanged: `universal/element.ts` `transformChildren` still calls
`transformNode(path)` with no info, and `shared/transform.ts` still emits
`escapeHTML(String(staticValue))` / `node.extra.raw`. rc.4 (published
2026-08-28, after our rc.3 bump) is therefore affected too.

The universal test fixture pins the behaviour rather than guarding against
it, and does so inconsistently:
`test/__universal_fixtures__/textInterpolation` expects

```js
_$createTextNode(`&nbsp;&lt;Hi&gt;&nbsp;`)                 // <span>&nbsp;&lt;Hi&gt;&nbsp;</span>
_$createComponent(Comp, { children: "\xA0<Hi>\xA0" })     // <Comp>&nbsp;&lt;Hi&gt;&nbsp;</Comp>
_$createTextNode(`Hi&lt;script>alert();&lt;/script>`)       // <span>Hi{"<script>alert();</script>"}</span>
```

The same JSX text reaches a component decoded and an element raw, and the
`injection` case shows the escaping was carried over from the DOM generator's
XSS guard, which has nothing to guard in a renderer without an HTML parser.
A PR has to update these three expectations; the component line is the
correct shape for all of them.

## Our side

Fix not applied yet. Two candidate workarounds, patch preferred:

- `bun patch @solidjs/babel-plugin` with the three-line change above; the
  diff doubles as the upstream PR. The patch must re-apply on every Solid rc
  bump (bun fails loudly when it does not), which is the reminder to check
  this file's status.
- A `Program.exit` visitor after the Solid plugin in `bundler.ts` that
  entity-decodes `createTextNode` literal arguments. Needs no dependency
  patch but couples to the plugin's output shape and needs a full entity
  decoder for the JSX-text path.

On `resolved`, remove the patch (or the visitor) and this file's mention of
it.
