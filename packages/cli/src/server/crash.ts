// Why a child ended abnormally, from the status its spawner reports: the
// signal that killed it (unix, by name), or a Windows exception status (a
// crash exits with an NTSTATUS of error severity, 0xC0000000 and up) as
// hex; null for a plain exit. Pure, so the bun commands (lib/util.ts) and
// the dev server (main.ts, a flux script) word it the same. The runtime
// names the fault itself on Windows (forge::process::report_crashes, a
// line on its stderr), so the status is only the token here. Bun keeps one
// byte of a Windows status (its u8 exit code, oven-sh/bun#33720), so under
// bun a Windows crash reads as a plain exit and this says nothing: the
// runtime's line above it does.

// The first NTSTATUS of severity "error": a process that dies of an
// exception exits with one of these.
const WINDOWS_ERROR_STATUS = 0xc0000000

/** The crash a child's exit status tells of, or null for a plain exit. */
export function crashReason(code: number | null, signal: string | number | null): string | null {
  // Bun reports a signal without a name (a Linux real-time one) by number.
  if (typeof signal === "number") return "signal " + signal
  if (signal) return signal
  // flux reports the status signed, bun and node unsigned.
  let status = code === null ? 0 : code >>> 0
  if (status >= WINDOWS_ERROR_STATUS) return "0x" + status.toString(16).toUpperCase()
  return null
}
