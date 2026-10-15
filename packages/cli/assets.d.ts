// Asset imports as the sol bundler (sol run / sol test / sol pack) inlines
// them. The import attribute decides the form, whatever the extension:
// `import data from "./pic.png" with { type: "binary" }` becomes the file's
// bytes as a Uint8Array, `import src from "./effect.glsl" with { type: "text" }`
// its UTF-8 contents as a string (src/bundle/bundler.ts, inlineImport). `.svg`
// is text with no attribute. This is the bundler's contract, not the
// runtime's: bare flux does not resolve these imports, so the declarations
// ship with the cli, and a project names it in its tsconfig `types` next to
// @solidrt/flux-types (the scaffold does).
//
// TypeScript types a module by its specifier, not by its attributes, so the
// declarations are per extension and these are the common ones. A project
// that inlines another extension declares it the same way in a .d.ts of its
// own. An ambient `declare module` reaches a program only from a non-module
// declaration file, so this file has no top-level import or export.

declare module "*.svg" {
  const content: string
  export default content
}
declare module "*.glsl" {
  const content: string
  export default content
}
declare module "*.vert" {
  const content: string
  export default content
}
declare module "*.frag" {
  const content: string
  export default content
}

declare module "*.png" {
  const bytes: Uint8Array
  export default bytes
}
declare module "*.jpg" {
  const bytes: Uint8Array
  export default bytes
}
declare module "*.jpeg" {
  const bytes: Uint8Array
  export default bytes
}
declare module "*.wav" {
  const bytes: Uint8Array
  export default bytes
}
declare module "*.opus" {
  const bytes: Uint8Array
  export default bytes
}
declare module "*.weba" {
  const bytes: Uint8Array
  export default bytes
}
declare module "*.glb" {
  const bytes: Uint8Array
  export default bytes
}
declare module "*.sol3m" {
  const bytes: Uint8Array
  export default bytes
}
declare module "*.wasm" {
  const bytes: Uint8Array
  export default bytes
}
