// The TLS fixtures (forge/src/tests/data/tls/) as the sol bundler inlines
// them with `with { type: "text" }`: PEM certificates and keys, and the
// key hashes in the `pin` form.

declare module "*.pem" {
  const content: string
  export default content
}
declare module "*.pin" {
  const content: string
  export default content
}
