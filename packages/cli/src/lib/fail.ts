// A fatal usage or configuration error. The throw-in-dev policy (CLAUDE.md,
// "API design"): a bad value fails the command instead of being papered
// over. It is thrown, not exited on, so a rule that calls `fail` is testable
// where it lives; main.ts catches it once and turns it into the message and
// exit code 1.
export class UsageError extends Error {}

export function fail(message: string): never {
  throw new UsageError(message)
}
