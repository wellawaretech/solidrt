// The `detached` attribute (okf/done/detached-attribute.md): `<view detached>`
// is the author's form of the detached element, and this pass rewrites it to
// the d-* wire kind (`<d-view>`) before the solid plugin runs, so the runtime
// and the native tree see the same tag as before. It is resolved at compile
// time because the tag name is the one thing that always reaches
// createElement: a spread on the element stops the compiler inlining static
// props, so a late `setProp(el, "detached", true)` would arrive after the node
// is built (and after its static children and its ref).
//
// resolveDetached is the decision on its own, with no Babel in it, so a flux
// test can import it; detachedAttribute is the Babel glue around it.

/** The tags with a detached form; the detached form is the tag with DETACHED_PREFIX. */
let PAIRED_TAGS = ["view", "rect", "oval", "line", "path", "texture", "text"]
/** The prefix of the detached wire kind: `d-view` is `<view detached>`. */
const DETACHED_PREFIX = "d-"

/** What a `detached` attribute on a host tag carries, as the pass reads it. */
export type DetachedValue = "true" | "false" | "other"
/** `tag` is the tag to emit (the attribute is always dropped); `error` is a compile error. */
export type DetachedResult = { tag: string } | { error: string }

/** The tag a host `tag` carrying a `detached` attribute of `value` compiles to, or the compile error. */
export function resolveDetached(tag: string, value: DetachedValue): DetachedResult {
  if (tag.startsWith(DETACHED_PREFIX) && PAIRED_TAGS.includes(tag.slice(DETACHED_PREFIX.length))) {
    return { error: `<${tag}> is the detached form already, write <${tag.slice(DETACHED_PREFIX.length)} detached>` }
  }
  if (!PAIRED_TAGS.includes(tag)) return { error: `<${tag}> has no detached form` }
  if (value === "true") return { tag: DETACHED_PREFIX + tag }
  if (value === "false") return { tag }
  return { error: "Detached is set once, as a literal attribute" }
}

// How the attribute's value reads: bare or {true}, {false}, or anything else.
function classify(value: any): DetachedValue {
  if (value == null) return "true"
  if (value.type === "JSXExpressionContainer" && value.expression.type === "BooleanLiteral") {
    return value.expression.value ? "true" : "false"
  }
  return "other"
}

// Babel plugin: rewrite a literal `detached` on a host tag to the d-* tag.
// The solid plugin transforms a JSXElement on enter and walks its nested JSX
// itself, so a JSXOpeningElement visitor of this plugin would miss nested
// elements; the whole file is rewritten from Program.enter instead, before
// any solid visitor runs. A component (an uppercase, member or namespaced
// name) takes `detached` as an ordinary prop and is left alone; a spread
// cannot be read at compile time and is ignored (applyProp in core throws
// when one carries the name).
export function detachedAttribute({ types: t }: { types: any }) {
  return {
    visitor: {
      Program: {
        enter(program: any) {
          program.traverse({
            JSXOpeningElement(p: any) {
              let name = p.node.name
              if (name.type !== "JSXIdentifier" || !/^[a-z]/.test(name.name)) return
              let attribute = p
                .get("attributes")
                .find(
                  (a: any) =>
                    a.node.type === "JSXAttribute" &&
                    a.node.name.type === "JSXIdentifier" &&
                    a.node.name.name === "detached",
                )
              if (!attribute) return
              let result = resolveDetached(name.name, classify(attribute.node.value))
              if ("error" in result) {
                // On a paired tag only the value can be wrong; otherwise the tag is.
                let at = PAIRED_TAGS.includes(name.name) ? attribute : p.get("name")
                throw at.buildCodeFrameError(result.error)
              }
              attribute.remove()
              p.node.name = t.jsxIdentifier(result.tag)
              let closing = p.parentPath.node.closingElement
              if (closing) closing.name = t.jsxIdentifier(result.tag)
            },
          })
        },
      },
    },
  }
}
