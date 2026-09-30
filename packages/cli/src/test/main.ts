import { cpSync, existsSync, mkdirSync, readdirSync, rmSync, statSync } from "node:fs"
import { basename, dirname, join, relative, resolve } from "node:path"
import { bundleWith, writeIsolates } from "../bundle/bundler"
import { appArgs, source, values } from "../lib/args"
import { collectAssets } from "../lib/project"
import { fail, requireBinary } from "../lib/util"
import { remapPositions } from "../server/remap"

// srt test: run the test files of a project on the runtime the code ships
// on (okf/plans/test-harness.md). Each `tests/*.test.ts` is bundled on its
// own and handed to a fresh test host process, one file after another. The
// binary is the host: it evaluates the file once to list the tests it
// registers through flux:test and once more for each test, in an engine of
// its own, and prints one JSON record per line on stdout. Which binary
// follows from the file's imports: `flux --test` for a flux program, the
// dev client (`solidrt-go --test`, headless and stepped by frames) for a
// file that imports the app runtime. This command is the only reporter.

// Where tests live, and what a test file is called: one tests/ folder per
// package or project, never beside the sources.
const TESTS_DIR = "tests"
const TEST_FILE = /\.test\.tsx?$/
// Folders never searched for a tests/ folder: dependencies and build
// output. Dot folders are skipped as well.
const SKIPPED_DIRS = ["node_modules", "dist", "target"]
// The module name the test host evaluates the file under, which is what
// its stack frames cite and so what the bundle's sourcemap is keyed by.
const ENTRY_MODULE = "main"
// What a stack frame inside the harness cites; those frames are dropped
// from a report, since they say nothing about the test.
const HARNESS_FRAME = "(flux:test:"
// The runtime modules the `flux` binary does not have: lattice's builtins
// and flux's gui layer. A test file whose bundle imports one runs on the
// dev client.
const APP_MODULE = /^srt:|^flux:(rendertree|camera|microphone|audio|gpu|spatial|video)$/
// Where an app test's staged bundle and its data root live, under the
// project's build output.
const STAGE_DIR = join("dist", "test")
// Marks a stdout line as a record of the test host's, not something a
// native library printed. It starts with a control character (the ASCII
// record separator) so that nothing prints it by accident.
const RECORD_PREFIX = "\x1esrt-test "
// How long one file may run before its process is stopped. A safety cap
// against a wedged process, which the host's own cap per test cannot see;
// not a wait.
const FILE_TIMEOUT_MS = 120_000

/** One test's result, with what its engine logged while it ran. */
type TestResult = {
  name: string
  ok: boolean
  durationMs: number
  error?: { message: string; stack: string }
  output: string[]
}
type RunRecord =
  | { type: "loaded"; tests: string[]; output: string[] }
  | { type: "failed"; message: string; output: string[] }
  | { type: "result"; result: TestResult }
  | { type: "done" }

type FileOutcome = {
  tests: TestResult[]
  /** What the file logged while it loaded, and what the process printed beside its records. */
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

type Bundled = { code: string; map: string | null; app: boolean }

// The runtime modules a bundle imports: what the bundler left external.
function runtimeImports(code: string): string[] {
  return [...code.matchAll(/^import\s(?:[^"']*?\sfrom\s*)?["']((?:flux|srt):[^"']+)["']/gm)].map((match) => match[1]!)
}

// A flux program's bundle: plain TypeScript, the runtime modules external.
async function bundleFlux(file: string): Promise<Bundled | string> {
  let result
  try {
    result = await Bun.build({
      entrypoints: [file],
      target: "browser",
      format: "esm",
      external: ["flux:*", "srt:*"],
      sourcemap: "external",
      throw: false,
    })
  } catch (e) {
    return String(e)
  }
  if (!result.success) return result.logs.map(String).join("\n")
  let entry = result.outputs.find((o) => o.kind === "entry-point")
  if (!entry) return "The bundler produced no output"
  let code = await entry.text()
  return { code, map: entry.sourcemap ? await entry.sourcemap.text() : null, app: runtimeImports(code).some((name) => APP_MODULE.test(name)) }
}

// An app's bundle (JSX through the Solid transform), staged like a render:
// the bundle, its isolates, the manifest and the project's assets under one
// root, which the client mounts. Returns the staged bundle's path.
async function stageApp(file: string): Promise<{ path: string; stage: string; map: string | null } | string> {
  let dir = workingDir(file)
  let project = existsSync(join(dir, "package.json")) ? dir : null
  let result = await bundleWith({ entry: file, dev: true, minify: false, project })
  if (!result) return "See the compile error above"
  let stage = join(dir, STAGE_DIR, basename(file).replace(/\.test\.tsx?$/, ""))
  rmSync(stage, { recursive: true, force: true })
  let path = join(stage, "test.srt.js")
  await Bun.write(path, result.code)
  writeIsolates(join(stage, "isolates"), result.isolates)
  await Bun.write(join(stage, "manifest.json"), result.manifest)
  for (let asset of collectAssets(project).assets) {
    let dest = join(stage, asset.path)
    mkdirSync(dirname(dest), { recursive: true })
    cpSync(join(project!, asset.path), dest)
  }
  return { path, stage, map: result.map }
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

async function runFile(file: string, filter: string | undefined, seed: number | undefined): Promise<FileOutcome> {
  let outcome: FileOutcome = { tests: [], output: [], error: null }
  let unbundled = (log: string) => {
    outcome.error = "The file failed to bundle"
    outcome.output = log.split("\n")
    return outcome
  }
  // JSX needs the Solid transform, so a .tsx file is an app test as it
  // stands; a .ts file is one when its bundle imports the app runtime.
  let bundled: Bundled | string = file.endsWith(".tsx") ? { code: "", map: null, app: true } : await bundleFlux(file)
  if (typeof bundled === "string") return unbundled(bundled)

  // The host's flags come ahead of the script; everything after `--` on
  // the command line reaches the file as its flux:process argv.
  let hostArgs = ["--test"]
  if (filter !== undefined) hostArgs.push("--filter", filter)
  if (seed !== undefined) hostArgs.push("--seed", String(seed))
  let proc
  let map = bundled.map
  if (bundled.app) {
    let staged = await stageApp(file)
    if (typeof staged === "string") return unbundled(staged)
    map = staged.map
    // A data root of the file's own, empty at the start; the calendar's
    // zone is fixed so that local-time methods read the same everywhere.
    let client = [...hostArgs, "--data-root", join(staged.stage, "data"), "--assets", staged.stage, staged.path, ...appArgs]
    proc = Bun.spawn([await testHost("solidrt-go"), ...client], {
      cwd: workingDir(file),
      env: { ...process.env, TZ: "UTC" },
      stdout: "pipe",
      stderr: "pipe",
    })
  } else {
    // "-": the bundle on stdin.
    proc = Bun.spawn([await testHost("flux"), ...hostArgs, "-", ...appArgs], {
      cwd: workingDir(file),
      stdin: new Blob([bundled.code]),
      stdout: "pipe",
      stderr: "pipe",
    })
  }
  let maps = map ? { [ENTRY_MODULE]: map } : null
  let timedOut = false
  let timer = setTimeout(() => {
    timedOut = true
    proc.kill()
  }, FILE_TIMEOUT_MS)

  // What a test logged arrives inside its record; a line that is no record
  // is something the process printed beside them.
  let remap = (text: string[]) => text.map((line) => remapPositions(line, maps))
  let failed: string[] = []
  let loaded = false
  let done = false
  let onLine = (line: string) => {
    if (!line.startsWith(RECORD_PREFIX)) {
      outcome.output.push(remapPositions(line, maps))
      return
    }
    let record = JSON.parse(line.slice(RECORD_PREFIX.length)) as RunRecord
    if (record.type === "result") {
      outcome.tests.push({ ...record.result, output: remap(record.result.output) })
    } else if (record.type === "loaded") {
      loaded = true
      outcome.output.push(...remap(record.output))
    } else if (record.type === "failed") {
      failed.push(record.message)
      outcome.output.push(...remap(record.output))
    } else {
      done = true
    }
  }
  let stderr: string[] = []
  await Promise.all([lines(proc.stdout, onLine), lines(proc.stderr, (line) => stderr.push(line)), proc.exited])
  clearTimeout(timer)

  // Source positions for the bundle's, and no frames of the harness.
  let cited = (text: string) =>
    remapPositions(text, maps)
      .split("\n")
      .filter((line) => line.trim() !== "" && !line.includes(HARNESS_FRAME))
      .join("\n")
  for (let test of outcome.tests) {
    if (!test.error) continue
    test.error.message = cited(test.error.message)
    test.error.stack = cited(test.error.stack)
  }

  let last = outcome.tests.at(-1)?.name
  let after = last === undefined ? "before its first test reported" : `after "${last}"`
  if (timedOut) outcome.error = `The file did not finish within ${FILE_TIMEOUT_MS / 1000} s and was stopped ${after}`
  else if (failed.length > 0) {
    // An uncaught error is in the output already; the host's own reason (a
    // file that timed out or never finished loading) is only here.
    let reason = cited(failed[0])
    let logged = outcome.output.join("\n").includes(reason)
    outcome.error = logged ? "The file failed before its tests ran" : `The file failed before its tests ran: ${reason}`
  }
  else if (!loaded) outcome.error = "The test host ended before the file had loaded"
  else if (!done) outcome.error = `The run ended ${after}, with tests left unreported`
  // stderr is the host's own log (the client's runtime lines): worth
  // reading when the file as a whole went wrong, noise when it ran.
  if (outcome.error) outcome.output.push(...stderr)
  while (outcome.output.at(-1)?.trim() === "") outcome.output.pop()
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

// The host binaries, resolved and checked on first use: a `flux` built
// without the `test` feature has no flux:test and takes `--test` for a
// script path, and a client built without it refuses the flag.
let hosts = new Map<string, string>()

async function testHost(name: "flux" | "solidrt-go"): Promise<string> {
  let known = hosts.get(name)
  if (known) return known
  let path = requireBinary(name)
  if (name === "flux") {
    let proc = Bun.spawn([path, "-"], {
      stdin: new Blob([`console.log(Flux.capabilities.includes("test"))`]),
      stdout: "pipe",
      stderr: "inherit",
    })
    let [out] = await Promise.all([new Response(proc.stdout).text(), proc.exited])
    if (out.trim() !== "true") {
      fail(`The flux binary at ${path} was built without flux:test. Build it with the test feature: make -C flux flux, in a SolidRT checkout.`)
    }
  }
  hosts.set(name, path)
  return path
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

  let filter = values.filter
  let seed = seedOption()
  let failed = 0
  let passed = 0
  let brokenFiles = 0
  for (let file of files) {
    let outcome = await runFile(file, filter, seed)
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
