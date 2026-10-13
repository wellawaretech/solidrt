// The project's own asset extensions, beyond the common ones @solidrt/cli
// declares: bytes with `with { type: "binary" }`, text with `with { type:
// "text" }`.
declare module "*.bin" {
  const bytes: Uint8Array
  export default bytes
}
declare module "*.txt" {
  const text: string
  export default text
}
