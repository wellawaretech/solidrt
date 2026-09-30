import { existsSync, readdirSync, statSync } from "node:fs"
import { basename, dirname, join, relative, resolve } from "node:path"
import { appArgs, source, values } from "../lib/args"
import { fail, requireBinary } from "../lib/util"
import { remapPositions } from "../server/remap"

// srt test: run the test files of a project on the runtime the code ships
// on (okf/plans/test-harness.md). Each `tests/*.test.ts` is bundled on its
// own and run in a fresh `flux` process, one file after another; the tests
// register through flux:test, a few lines appended to the bundle run them
// with its public `run`, and every result comes back as one JSON line on
// stdout. This command is the only reporter.

// Where tests live, and what a test file is called: one tests/ folder per
// package or project, never beside the sources.
const TESTS_DIR = "tests"
const TEST_FILE = /\.test\.tsx?$/
// Folders never searched for a tests/ folder: dependencies and build
// output. Dot folders are skipped as well.
const SKIPPED_DIRS = ["node_modules", "dist", "target"]
// The module name flux gives the script it runs, which is what its stack
// frames cite and so what the bundle's sourcemap is keyed by.
const ENTRY_MODULE = "main"
// What a stack frame inside the harness cites; those frames are dropped
// from a report, since they say nothing about the test.
const HARNESS_FRAME = "(flux:test:"
// Marks a stdout line as a record of this command's, not the test's own
// output. It starts with a control character (the ASCII record separator)
// so that no test prints it by accident.
const RECORD_PREFIX = "\x1esrt-test "
// How long one file may run before its process is stopped. A safety cap
// against a synchronous loop or a wedged process, which the per-test cap
// inside flux:test cannot see; not a wait.
const FILE_TIMEOUT_MS = 120_000

type TestResult = { name: string; ok: boolean; durationMs: number; error?: { message: string; stack: string } }
type RunRecord = { type: "loaded" } | { type: "result"; result: TestResult } | { type: "done" }

/** One test's result with what it printed while it ran. */
type Reported = TestResult & { output: string[] }

type FileOutcome = {
  tests: Reported[]
  /** What the file printed outside any test: while loading, or after the last one. */
  output: string[]
  /** Why the file as a whole failed, whatever its tests reported. */
  error: string | null
}

// -- Discovery --

function testFilesIn(dir: string): string[] {
  return readdirSync(dir)
    .filter((name) => TEST_FILE.test(name))
    .sort()
    .map((name) => join(dir, name))
}

function discover(dir: string): string[] {
  if (basename(dir) === TESTS_DIR) return testFilesIn(dir)
  let files: string[] = []
  for (let entry of readdirSync(dir, { withFileTypes: true }).sort((a, b) => (a.name < b.name ? -1 : 1))) {
    if (!entry.isDirectory() || entry.name.startsWith(".") || SKIPPED_DIRS.includes(entry.name)) continue
    files.push(...discover(join(dir, entry.name)))
  }
  return files
}

// -- One file --

// What is appended to a test file's bundle: run what the file registered
// and print each result as a record. In a block, so its names cannot meet
// the file's own top-level ones.
function runnerSource(filter: string | undefined, seed: number | undefined): string {
  let print = (record: string) => `console.log(${JSON.stringify(RECORD_PREFIX)} + JSON.stringify(${record}))`
  return [
    `import { run as __srtTestRun } from "flux:test"`,
    `{`,
    `  ${print(`{ type: "loaded" }`)}`,
    `  for await (let result of __srtTestRun(${JSON.stringify({ filter, seed })})) ${print(`{ type: "result", result }`)}`,
    `  ${print(`{ type: "done" }`)}`,
    `}`,
    ``,
  ].join("\n")
}

async function bundle(file: string): Promise<{ code: string; map: string | null } | string> {
  let result
  try {
    result = await Bun.build({
      entrypoints: [file],
      target: "browser",
      format: "esm",
      external: ["flux:*"],
      sourcemap: "external",
      throw: false,
    })
  } catch (e) {
    return String(e)
  }
  if (!result.success) return result.logs.map(String).join("\n")
  let entry = result.outputs.find((o) => o.kind === "entry-point")
  if (!entry) return "The bundler produced no output"
  return { code: await entry.text(), map: entry.sourcemap ? await entry.sourcemap.text() : null }
}

// The directory a test file runs in: the package or project that holds its
// tests/ folder, so a relative path in a test means the same wherever
// `srt test` was started.
function workingDir(file: string): string {
  let dir = dirname(file)
  return basename(dir) === TESTS_DIR ? dirname(dir) : dir
}

async function lines(stream: ReadableStream<Uint8Array>, onLine: (line: string) => void): Promise<void> {
  let decoder = new TextDecoder()
  let rest = ""
  for await (let chunk of stream) {
    rest += decoder.decode(chunk, { stream: true })
    let parts = rest.split("\n")
    rest = parts.pop()!
    for (let line of parts) onLine(line)
  }
  if (rest !== "") onLine(rest)
}

async function runFile(flux: string, file: string, filter: string | undefined, seed: number | undefined): Promise<FileOutcome> {
  let outcome: FileOutcome = { tests: [], output: [], error: null }
  if (file.endsWith(".tsx")) {
    outcome.error = "App tests (.test.tsx, on srt:test) are not supported yet"
    return outcome
  }
  let bundled = await bundle(file)
  if (typeof bundled === "string") {
    outcome.error = "The file failed to bundle"
    outcome.output = bundled.split("\n")
    return outcome
  }
  let maps = bundled.map ? { [ENTRY_MODULE]: bundled.map } : null

  // Everything after `--` on the command line reaches the file as its
  // flux:process argv.
  let proc = Bun.spawn([flux, "-", ...appArgs], {
    cwd: workingDir(file),
    stdin: new Blob([bundled.code + "\n" + runnerSource(filter, seed)]),
    stdout: "pipe",
    stderr: "pipe",
  })
  let timedOut = false
  let timer = setTimeout(() => {
    timedOut = true
    proc.kill()
  }, FILE_TIMEOUT_MS)

  // Lines between two records are what the test between them printed.
  let pending: string[] = []
  let loaded = false
  let done = false
  let onLine = (line: string) => {
    if (!line.startsWith(RECORD_PREFIX)) {
      pending.push(remapPositions(line, maps))
      return
    }
    let record = JSON.parse(line.slice(RECORD_PREFIX.length)) as RunRecord
    if (record.type === "result") {
      outcome.tests.push({ ...record.result, output: pending })
    } else {
      if (record.type === "loaded") loaded = true
      else done = true
      outcome.output.push(...pending)
    }
    pending = []
  }
  let stderr: string[] = []
  let [, , code] = await Promise.all([
    lines(proc.stdout, onLine),
    lines(proc.stderr, (line) => stderr.push(line)),
    proc.exited,
  ])
  clearTimeout(timer)
  outcome.output.push(...pending, ...stderr)
  while (outcome.output.at(-1)?.trim() === "") outcome.output.pop()

  for (let test of outcome.tests) {
    if (!test.error) continue
    test.error.stack = remapPositions(test.error.stack, maps)
      .split("\n")
      .filter((frame) => frame.trim() !== "" && !frame.includes(HARNESS_FRAME))
      .join("\n")
  }

  let last = outcome.tests.at(-1)?.name
  let after = last === undefined ? "before its first test reported" : `after "${last}"`
  if (timedOut) outcome.error = `The file did not finish within ${FILE_TIMEOUT_MS / 1000} s and was stopped ${after}`
  else if (!loaded) outcome.error = "The file failed before its tests ran"
  else if (!done) outcome.error = `The run ended ${after}, with tests left unreported`
  else if (code !== 0) outcome.error = "The file reported an uncaught error outside its tests"
  return outcome
}

// -- Reporting --

function indent(text: string[], by: string): string {
  return text.map((line) => by + line).join("\n")
}

function count(n: number, what: string): string {
  return `${n} ${what}`
}

function report(name: string, outcome: FileOutcome) {
  let failed = outcome.tests.filter((t) => !t.ok).length
  let passed = outcome.tests.length - failed
  let counts = [failed > 0 ? count(failed, "failed") : "", count(passed, "passed")].filter(Boolean).join(", ")
  console.log(`${name}: ${outcome.error ? "FAILED" : counts}`)
  for (let test of outcome.tests) {
    if (test.ok && test.output.length === 0) continue
    console.log(`\n  ${test.ok ? "Passed" : "FAILED"}: ${test.name}`)
    if (test.error) {
      console.log(indent(test.error.message.split("\n"), "    "))
      if (test.error.stack !== "") console.log(indent(test.error.stack.split("\n"), "    "))
    }
    if (test.output.length > 0) console.log(`    Output:\n${indent(test.output, "      ")}`)
  }
  if (outcome.error) {
    console.log(`\n  ${outcome.error}${outcome.tests.length > 0 ? ` (${counts} until then)` : ""}`)
    if (outcome.output.length > 0) console.log(indent(outcome.output, "    "))
  } else if (outcome.output.length > 0) {
    console.log(`\n  Output outside the tests:\n${indent(outcome.output, "    ")}`)
  }
  if (outcome.error || outcome.tests.some((t) => !t.ok || t.output.length > 0) || outcome.output.length > 0) console.log("")
}

// A `flux` binary built without the `test` feature has no flux:test, and
// the import would fail once per file with a resolver error; ask it once.
async function requireTestModule(flux: string) {
  let proc = Bun.spawn([flux, "-"], {
    stdin: new Blob([`console.log(Flux.capabilities.includes("test"))`]),
    stdout: "pipe",
    stderr: "inherit",
  })
  let [out] = await Promise.all([new Response(proc.stdout).text(), proc.exited])
  if (out.trim() !== "true") {
    fail(`The flux binary at ${flux} was built without flux:test. Build it with the test feature: make -C flux flux, in a SolidRT checkout.`)
  }
}

// --seed: the number Math.random starts from in every test. Without it the
// tests run on flux:test's own fixed seed, so a run is the same either way.
function seedOption(): number | undefined {
  let raw = values.seed
  if (raw === undefined) return undefined
  let seed = Number(raw)
  if (!/^\d+$/.test(raw) || !Number.isSafeInteger(seed)) fail(`Invalid --seed value "${raw}": expected a non-negative integer`)
  return seed
}

export async function main() {
  let target = resolve(source ?? ".")
  if (!existsSync(target)) fail(`No such file or folder: ${source} (resolved from ${process.cwd()})`)
  let files = statSync(target).isDirectory() ? discover(target) : [target]
  if (files.length === 0) {
    fail(`No test files found under ${target} (looked for ${TESTS_DIR}/*.test.ts and ${TESTS_DIR}/*.test.tsx)`)
  }

  let flux = requireBinary("flux")
  await requireTestModule(flux)

  let filter = values.filter
  let seed = seedOption()
  let failed = 0
  let passed = 0
  let brokenFiles = 0
  for (let file of files) {
    let outcome = await runFile(flux, file, filter, seed)
    // With a filter, a file none of whose tests match has nothing to say.
    if (filter !== undefined && outcome.tests.length === 0 && !outcome.error) continue
    report(relative(process.cwd(), file), outcome)
    failed += outcome.tests.filter((t) => !t.ok).length
    passed += outcome.tests.filter((t) => t.ok).length
    if (outcome.error) brokenFiles++
  }

  if (failed + passed === 0 && brokenFiles === 0) {
    fail(filter === undefined ? "The test files register no tests" : `No test name contains "${filter}"`)
  }
  let parts = [failed > 0 ? count(failed, "failed") : "", count(passed, "passed")].filter(Boolean)
  if (brokenFiles > 0) parts.push(`${count(brokenFiles, brokenFiles === 1 ? "file" : "files")} did not complete`)
  // A run on another seed says which, so a failure in it can be run again.
  let notes = [count(files.length, files.length === 1 ? "file" : "files")]
  if (seed !== undefined) notes.push(`seed ${seed}`)
  console.log(`Tests: ${parts.join(", ")} (${notes.join(", ")})`)
  process.exit(failed > 0 || brokenFiles > 0 ? 1 : 0)
}
