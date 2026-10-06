// A fatal usage or configuration error: the message, then exit. The
// throw-in-dev policy (CLAUDE.md, "API design"): a bad value fails the
// command instead of being papered over.
export function fail(message: string): never {
  console.error(message)
  process.exit(1)
}
