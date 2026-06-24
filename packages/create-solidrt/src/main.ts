// `bun create solidrt <dir>` entry point. A thin wrapper that forwards to
// `srt init` in @solidrt/cli, so there is one source of truth for what a new
// SolidRT project looks like. We shell out via `bun x` rather than depend on
// @solidrt/cli directly, to avoid pulling its native-binary tree into the
// scaffolder. The empty/existing-folder guard lives in `srt init`.

let dir = process.argv[2]
if (!dir || dir.startsWith("-")) {
  console.error("Usage: bun create solidrt <dir>")
  process.exit(1)
}

let proc = Bun.spawnSync(["bun", "x", "@solidrt/cli", "init", dir], {
  stdin: "inherit",
  stdout: "inherit",
  stderr: "inherit",
})
process.exit(proc.exitCode ?? 0)