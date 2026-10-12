// The project's asset imports: bytes with `with { type: "binary" }`, text
// with `with { type: "text" }`.
declare module "*.bin" {
  const bytes: Uint8Array
  export default bytes
}
declare module "*.txt" {
  const text: string
  export default text
}
