// `bun create solidrt <dir>` entry point. A thin wrapper that forwards to
// `srt init` in @solidrt/cli, so there is one source of truth for what a new
// SolidRT project looks like. We shell out via `bun x` rather than depend on
// @solidrt/cli directly, to avoid pulling its native-binary tree into the
// scaffolder. The empty/existing-folder guard lives in `srt init`.
import pkg from "../package.json" with { type: "json" }

let dir = process.argv[2]
if (!dir || dir.startsWith("-")) {
  console.error("Usage: bun create solidrt <dir>")
  process.exit(1)
}

// Pin the forwarded cli to our own version. create-solidrt and @solidrt/cli are
// published in lockstep, so this keeps the scaffolder matched to the version the
// user just fetched, and forces `bun x` to resolve that exact version instead of
// silently reusing a stale bare-name cache entry. Fall back to `latest` for the
// unpublished 0.0.0 workspace version, which has no npm release to pin to.
let cli = pkg.version === "0.0.0" ? "@solidrt/cli@latest" : `@solidrt/cli@${pkg.version}`

// Forward any trailing flags (e.g. --with @solidrt/components) to `srt init` untouched.
let extra = process.argv.slice(3)
let proc = Bun.spawnSync(["bun", "x", cli, "init", dir, ...extra], {
  stdin: "inherit",
  stdout: "inherit",
  stderr: "inherit",
})
process.exit(proc.exitCode ?? 0)