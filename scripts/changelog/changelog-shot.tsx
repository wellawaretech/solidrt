// Renders the newest release's sections of CHANGELOG.md as a PNG:
//
//   ./changelog-shot [output-dir] [width]
//
// The app captures itself and writes the file - render the card, snapshot the
// content node, encode, write, exit. The capture is the node's own box, so the
// image is cropped to the content with no measuring pass and no image tooling.
//
// Playback gives an offscreen surface, a virtual clock and the runtime's
// embedded fonts, and lays out at scale 1, so the same changelog produces the
// same pixels every run and on every machine - which a desktop screenshot never
// does.
//
// The changelog is inlined at bundle time. The runtime chdirs into its own data
// sandbox before app code runs, so a relative read at runtime would not find
// the repo file, and the output directory arrives as an absolute path in argv.
//
// A bullet is one flowing paragraph, as in the source: one <text> whose
// styled runs (a bold lead-in, `inline code`, a [platform] label) are <span>
// children, wrapped as a whole by the text engine.
import { createLinearGradient, encodeImage, exit, render } from "@solidrt/core"
import { captureSnapshot } from "@solidrt/core/gpu"
import { file } from "flux:fs"
import { argv } from "flux:process"
import source from "../../CHANGELOG.md" with { type: "text" }

const SECTIONS = ["Fixes", "Features", "API", "Breaking changes"]
const PADDING = 50

type RunKind = "normal" | "strong" | "code" | "strong-code" | "label"

type Run = {
  kind: RunKind
  text: string
}

type Section = {
  title: string
  // Each bullet is a list of styled runs.
  bullets: Run[][]
}

// The newest release runs from the first "## " heading to the next one. The
// authoring rules at the top of the file are an HTML comment and contain no
// headings, so no need to skip them.
function newestRelease(text: string): { title: string, lines: string[] } {
  let lines = text.split("\n")
  let start = lines.findIndex(l => l.startsWith("## "))
  if (start < 0) return { title: "", lines: [] }
  let after = lines.findIndex((l, i) => i > start && l.startsWith("## "))
  return {
    title: lines[start]!.slice(3).trim(),
    lines: lines.slice(start + 1, after < 0 ? lines.length : after),
  }
}

// Split a bullet into styled runs: a leading [platform] label, **strong** spans
// and `code` spans, with everything else as body text. Whitespace stays in the
// runs; the text engine wraps on it.
function inlineRuns(text: string): Run[] {
  let runs: Run[] = []
  let push = (kind: RunKind, value: string) => {
    if (value !== "") runs.push({ kind, text: value })
  }
  let label = /^\[([^\]]+)\]/.exec(text)
  if (label) {
    push("label", label[0])
    text = text.slice(label[0].length)
  }
  // Code spans nest inside strong spans ("**`argv` reshaped**"), so strong
  // splits first and each side parses its own code spans; a code span may also
  // contain asterisks ("`setShader*`"), which is why strong cannot be the
  // inner pass.
  let code = (part: string, kind: "normal" | "strong") => {
    let index = 0
    let span = /`([^`]+)`/g
    let match: RegExpExecArray | null
    while ((match = span.exec(part)) !== null) {
      push(kind, part.slice(index, match.index))
      push(kind === "strong" ? "strong-code" : "code", match[1]!)
      index = match.index + match[0].length
    }
    push(kind, part.slice(index))
  }
  let index = 0
  let strong = /\*\*(.+?)\*\*/g
  let match: RegExpExecArray | null
  while ((match = strong.exec(text)) !== null) {
    code(text.slice(index, match.index), "normal")
    code(match[1]!, "strong")
    index = match.index + match[0].length
  }
  code(text.slice(index), "normal")
  return runs
}

// Only the sections named in SECTIONS, in the order the file lists them. A
// bullet may wrap across lines; continuation lines are indented.
function parseSections(lines: string[]): Section[] {
  let sections: Section[] = []
  let current: Section | null = null
  let pending: string[] = []
  let flushBullet = () => {
    if (current && pending.length > 0) current.bullets.push(inlineRuns(pending.join(" ")))
    pending = []
  }
  for (let line of lines) {
    if (line.startsWith("### ")) {
      flushBullet()
      let title = line.slice(4).trim()
      current = SECTIONS.includes(title) ? { title, bullets: [] } : null
      if (current) sections.push(current)
    } else if (line.startsWith("- ")) {
      flushBullet()
      pending = [line.slice(2).trim()]
    } else if (pending.length > 0 && line.trim() !== "") {
      pending.push(line.trim())
    } else {
      flushBullet()
    }
  }
  flushBullet()
  return sections
}

// Sizes are for a 1:1 canvas: playback pins the display scale to 1, so these
// are output pixels and do not shift with the host's DPI setting. Body text is
// the paragraph's own style; every other kind is a span override on it.
function Styled(props: { run: Run }) {
  let run = props.run
  if (run.kind === "strong") {
    return <span fontWeight={700} color="#e8ecf5">{run.text}</span>
  }
  if (run.kind === "code") {
    return <span fontFamily="mono" fontSize={16} lineHeight={1.55} color="#9fc0ff">{run.text}</span>
  }
  if (run.kind === "strong-code") {
    return <span fontFamily="mono" fontSize={16} fontWeight={700} lineHeight={1.55} color="#9fc0ff">{run.text}</span>
  }
  if (run.kind === "label") {
    return <span fontSize={16} fontWeight={600} color="#7f9bd8">{run.text}</span>
  }
  return <>{run.text}</>
}

function Bullet(props: { runs: Run[] }) {
  return (
    <view flexDirection="row" gap={12} alignItems="flex-start">
      <oval width={6} height={6} color="#4f6bb0" marginTop={10} />
      <text flex={1} fontSize={17} lineHeight={1.45} color="#a3aec7">
        {props.runs.map(run => <Styled run={run} />)}
      </text>
    </view>
  )
}

function App() {
  let backgroundColor = createLinearGradient(0, 0, 1, 1, [
    { offset: 0, color: "#080b16" },
    { offset: 1, color: "#1d2a52" },
  ])

  let release = newestRelease(source)
  let sections = parseSections(release.lines)

  // One shot, after the first paint: a capture is serviced by the paint walk,
  // so it needs a frame to have happened. The image is named for the release
  // because only the changelog knows which one this is; the caller owns the
  // directory.
  let content!: { id: number }
  requestAnimationFrame(async () => {
    let dir = argv[0]
    if (!dir) {
      console.error("changelog-shot: no output directory (pass one after --)")
      exit()
      return
    }
    let path = `${dir}/changelog-${release.title.split(" ")[0]}.png`
    try {
      let snap = await captureSnapshot(content.id)
      await file(path).write(encodeImage(snap))
      console.log(`changelog-shot: wrote ${path} (${snap.width}x${snap.height})`)
    } catch (err) {
      console.error(`changelog-shot: ${err}`)
    }
    exit()
  })

  return (
    <window flexDirection="column">
      <view flexDirection="column">
        {/* The background belongs INSIDE the captured node: a sibling behind it
            is not part of the node, and the card would capture transparent. */}
        <view ref={n => (content = n)} flexDirection="column" gap={22} padding={PADDING}>
          <d-rect color={backgroundColor} />
          <text fontSize={16} fontWeight={600} color="#6f86c4">{release.title}</text>
          {sections.map(section => (
            <view flexDirection="column" gap={9}>
              <text fontSize={24} fontWeight={700} color="#e8ecf5">{section.title}</text>
              {/* Bullets run together as a list; only sections get breathing room. */}
              <view flexDirection="column">
                {section.bullets.map(runs => <Bullet runs={runs} />)}
              </view>
            </view>
          ))}
        </view>
      </view>
    </window>
  )
}

render(() => <App />)
