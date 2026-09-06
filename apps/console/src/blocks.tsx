// The transcript's vocabulary. A block is plain data (`Block`), and each kind
// has one renderer in BLOCKS: adding a content type is one union member and
// one entry there, and the chat never looks inside a block. A renderer gets
// the entry and the chat it sits in (`BlockContext`): the live records a
// choice block lists, the outcome marks, and the follow-up commands a press
// in a block starts. Live data (the slots) is read inside a renderer's JSX,
// never in its body: a read in the body would rebuild the block on every
// poll.
import { For, Show } from "@solidrt/core"
import { Button, Icon, Image, Pressable, Text, View, space, theme, type PressState } from "@solidrt/components"
import type { Slot } from "./servers"
import type { LogLine, Shot } from "./control"
import { CHECK_ICON, CROSS_ICON, TAP_TARGET, focusRing } from "./ui"

// Width a client's screenshot is shown at; the height follows the picture.
const SHOT_WIDTH = 320

// How wide a bubble is allowed to get. Wide enough for a screenshot and its
// padding, narrow enough that a reply does not run the width of the pane.
const BUBBLE_MAX_WIDTH = 420

// How much of a debug command's result a Json block shows, in lines.
const JSON_LINES = 60

// Indent per level of a Tree block's rows.
const TREE_INDENT = 12

/** One node of a Tree block: the tree flattened, parents before children. */
export type TreeRow = {
  id: number
  depth: number
  kind: string
  text?: string
  x: number
  y: number
  width: number
  height: number
  /** Children the block's depth cut off. */
  childCount?: number
}

// `command` is the only block you author, and it carries its own outcome as
// a mark (see `BlockContext.mark`), so a command whose result is nothing to
// look at needs no reply repeating what was pressed.
export type Block =
  | { kind: "command"; text: string }
  | { kind: "text"; lines: string[]; tone?: "danger" }
  | { kind: "slots" }
  | ({ kind: "shot" } & Shot)
  | { kind: "logs"; lines: LogLine[] }
  | { kind: "stats"; stats: any }
  | { kind: "tree"; rows: TreeRow[]; hidden: number }
  | { kind: "debug"; names: string[] }
  | { kind: "json"; value: unknown }
  | { kind: "gpu"; gpu: any }

export type Entry = Block & { id: number }

/** What a renderer may ask of the chat its block sits in. */
export type BlockContext = {
  /** The client slots on this machine, live. */
  slots: () => Slot[]
  /** A command's outcome as an icon, or null while it runs or when it never
   * acted. */
  mark: (id: number) => string | null
  /** Start a client in a slot. */
  start: (slot: number) => void
  /** Blow a shot up over the window. */
  zoom: (shot: Shot) => void
  /** Snapshot one node of the tree; `label` names it in the transcript. */
  snapshot: (nodeId: number, label: string) => void
  /** Call a debug command by name. */
  call: (name: string) => void
  /** Read a GPU texture back. */
  texture: (id: number) => void
}

// One logged line: its level for anything but a plain log, then the text,
// with a repeat count when the server collapsed identical lines.
function logText(line: LogLine): string {
  let prefix = line.level === "log" ? "" : `[${line.level}] `
  let repeats = line.repeats ? ` (x${line.repeats})` : ""
  return `${prefix}${line.text.trimEnd()}${repeats}`
}

// The statistics as lines a person reads: the live figures, the window's
// summary and its worst frame, then the tree and GPU health. Field names as
// mcp/main.ts get_stats documents them; a figure a runtime does not report
// reads as "?".
function statsLines(s: any): string[] {
  let n = (v: unknown) => (typeof v === "number" ? String(v) : "?")
  let mb = (v: unknown) => (typeof v === "number" ? `${Math.round(v / 1048576)} MB` : "?")
  let lines = [`${n(s.fps)} fps, frame ${n(s.frameMs)} ms, cpu ${n(s.cpuPct)}%, memory ${mb(s.memBytes)}`]
  let w = s.window
  if (w) {
    let span = `${n(w.windowMs / 1000)} s`
    if (w.frames === 0) lines.push(`Nothing rebuilt in the last ${span}`)
    else
      lines.push(
        `Last ${span}: ${n(w.frames)} frames, p50 ${n(w.p50Ms)} p95 ${n(w.p95Ms)} max ${n(w.maxMs)} ms, ` +
          `${n(w.slowFrames)} slow (period ${n(w.periodMs)} ms)`,
      )
    let x = w.worst
    if (x)
      lines.push(
        `Worst ${n(x.totalMs)} ms, ${n(Math.round(x.ageMs / 100) / 10)} s ago: js ${n(x.jsMs)} layout ${n(x.layoutMs)} ` +
          `paint ${n(x.paintMs)} hover ${n(x.hoverMs)}; ${n(x.paraShapes)} shaped, ${n(x.dirtiedNodes)} dirtied, ` +
          `cache ${n(x.cacheHits)}/${n(x.cacheGets)}, ${n(x.nodesPainted)} painted`,
      )
    lines.push(
      `GPU frame ${n(w.gpuFrameExecMsPerFrame)} ms, ${n(w.gpuPassesPerFrame)} passes/frame, ` +
        `raster ${n(w.rasterCmdMsPerSec)} ms/s`,
    )
  }
  lines.push(
    `${n(s.nodes)} nodes (${n(s.mountedNodes)} mounted, ${n(s.orphanNodes)} orphans), ${n(s.textures)} textures, ` +
      `raster queue ${n(s.rasterQueue)}, fence timeouts ${n(s.fenceTimeouts)}`,
  )
  return lines
}

// One tree row as it reads: the kind, the text a text node holds, its box,
// and how many children the depth cut off.
function treeText(row: TreeRow): string {
  let text = row.text !== undefined ? ` "${row.text.length > 24 ? row.text.slice(0, 24) + "..." : row.text}"` : ""
  let box = `${Math.round(row.x)},${Math.round(row.y)} ${Math.round(row.width)}x${Math.round(row.height)}`
  let more = row.childCount ? ` +${row.childCount}` : ""
  return `${row.kind}${text} ${box}${more}`
}

// A pressable line of a listing block (a tree node, a texture): caption
// text, indented by its level, lit on hover.
function Row(props: { text: string; indent?: number; onPress: () => void }) {
  return (
    <Pressable
      focusable
      onPress={props.onPress}
      layout={{ paddingTop: 2, paddingBottom: 2, paddingLeft: (props.indent ?? 0) * TREE_INDENT }}
      style={(state: PressState) => ({
        backgroundColor: state.hovered ? theme.color.overlayHover : "transparent",
        borderRadius: theme.radius.sm,
        ...focusRing(state.focused, theme.radius.sm),
      })}
    >
      <Text variant="caption">{props.text}</Text>
    </Pressable>
  )
}

// One turn. Yours sit right and filled, the console's left on a surface,
// which is the whole of what makes this read as a chat.
function Bubble(props: { mine?: boolean; children?: any }) {
  return (
    <View
      layout={{
        alignSelf: props.mine ? "flex-end" : "flex-start",
        maxWidth: BUBBLE_MAX_WIDTH,
        flexDirection: "column",
        gap: space("sm"),
        padding: space("md"),
      }}
      style={{
        backgroundColor: props.mine ? theme.color.primary : theme.color.surface,
        borderRadius: theme.radius.lg,
      }}
    >
      {props.children}
    </View>
  )
}

// Plain lines in a bubble: what most replies are made of.
function Lines(props: { lines: string[]; tone?: "danger"; small?: boolean }) {
  return (
    <Bubble>
      <For each={props.lines}>
        {(line: string) => (
          <Text
            variant={props.small ? "caption" : undefined}
            muted={props.small && props.tone !== "danger"}
            color={props.tone === "danger" ? "danger" : undefined}
          >
            {line}
          </Text>
        )}
      </For>
    </Bubble>
  )
}

type Renderer<K extends Block["kind"]> = (entry: Extract<Entry, { kind: K }>, ctx: BlockContext) => any

const BLOCKS: { [K in Block["kind"]]: Renderer<K> } = {
  command: (entry, ctx) => (
    <Bubble mine>
      <View layout={{ flexDirection: "row", alignItems: "center", gap: space("sm") }}>
        <Text color="onPrimary">{entry.text}</Text>
        <Show when={ctx.mark(entry.id)}>
          {(icon) => <Icon src={icon()} size={15} color={theme.color.onPrimary} />}
        </Show>
      </View>
    </Bubble>
  ),
  text: (entry) => <Lines lines={entry.lines} tone={entry.tone} />,
  // One small button per slot, its fill the state: accent free, danger in
  // use. A held slot still spawns: the runtime, not the console, decides
  // what two clients on one tree means. Sized (not stretched) but pinned
  // to the tap target rather than the size preset, so ten still fit a row.
  slots: (_entry, ctx) => (
    <Bubble>
      <View layout={{ flexDirection: "row", gap: space("sm"), flexWrap: "wrap" }}>
        <For each={ctx.slots()} keyed={(slot: Slot) => slot.index}>
          {(slot) => (
            <Button
              size="sm"
              variant={slot().held ? "danger" : "primary"}
              layout={{ minWidth: TAP_TARGET }}
              onPress={() => ctx.start(slot().index)}
            >
              {String(slot().index)}
            </Button>
          )}
        </For>
      </View>
    </Bubble>
  ),
  // A shot in the transcript is a thumbnail: readable enough to tell one
  // screenshot from the next, too small to read the app in. Pressing it
  // blows it up over the window.
  shot: (entry, ctx) => (
    <Bubble>
      <Pressable
        focusable
        onPress={() => ctx.zoom(entry)}
        style={(state: PressState) => focusRing(state.focused)}
      >
        <Image
          src={entry.png}
          fit="contain"
          layout={{
            width: SHOT_WIDTH,
            height: Math.round((SHOT_WIDTH * entry.height) / entry.width),
          }}
        />
      </Pressable>
    </Bubble>
  ),
  logs: (entry) => (
    <Bubble>
      <View layout={{ flexDirection: "column", gap: 2 }}>
        <For each={entry.lines} fallback={<Text muted>No output yet</Text>}>
          {(line: LogLine) => (
            <Text
              variant="caption"
              color={line.level === "error" ? "danger" : undefined}
              muted={line.level !== "error"}
            >
              {logText(line)}
            </Text>
          )}
        </For>
      </View>
    </Bubble>
  ),
  stats: (entry) => <Lines lines={statsLines(entry.stats)} small />,
  // The tree's top as rows; pressing one snapshots that node.
  tree: (entry, ctx) => (
    <Bubble>
      <View layout={{ flexDirection: "column" }}>
        <For each={entry.rows}>
          {(row: TreeRow) => (
            <Row
              text={treeText(row)}
              indent={row.depth}
              onPress={() => ctx.snapshot(row.id, `${row.kind} #${row.id}`)}
            />
          )}
        </For>
        <Show when={entry.hidden > 0}>
          <Text variant="caption" muted>
            {`${entry.hidden} more rows not shown`}
          </Text>
        </Show>
      </View>
    </Bubble>
  ),
  // The app's debug commands as buttons; pressing one calls it without an
  // argument and its result comes back as a Json block.
  debug: (entry, ctx) => (
    <Bubble>
      <View layout={{ flexDirection: "row", gap: space("sm"), flexWrap: "wrap" }}>
        <For each={entry.names} fallback={<Text muted>No debug commands registered</Text>}>
          {(name: string) => (
            <Button size="sm" variant="secondary" onPress={() => ctx.call(name)}>
              {name}
            </Button>
          )}
        </For>
      </View>
    </Bubble>
  ),
  json: (entry) => {
    let lines = JSON.stringify(entry.value, null, 2)?.split("\n") ?? ["undefined"]
    let shown = lines.slice(0, JSON_LINES)
    if (lines.length > JSON_LINES) shown.push(`... ${lines.length - JSON_LINES} more lines`)
    return <Lines lines={shown} small />
  },
  // The inventory: a line of counts, then the textures as rows (pressing
  // one reads it back as a picture) and the buffers as lines.
  gpu: (entry, ctx) => {
    let textures: any[] = Array.isArray(entry.gpu?.textures) ? entry.gpu.textures : []
    let buffers: any[] = Array.isArray(entry.gpu?.buffers) ? entry.gpu.buffers : []
    let pipelines: any[] = Array.isArray(entry.gpu?.pipelines) ? entry.gpu.pipelines : []
    return (
      <Bubble>
        <View layout={{ flexDirection: "column", gap: 2 }}>
          <Text variant="caption" muted>
            {`${textures.length} textures, ${buffers.length} buffers, ${pipelines.length} pipelines`}
          </Text>
          <For each={textures}>
            {(texture: any) => (
              <Row
                text={`texture #${texture.id} ${texture.width}x${texture.height} ${texture.format ?? ""}${texture.target ? " target" : ""}`}
                onPress={() => ctx.texture(texture.id)}
              />
            )}
          </For>
          <For each={buffers}>
            {(buffer: any) => (
              <Text variant="caption" muted>
                {`buffer #${buffer.id} ${buffer.byteLength} bytes`}
              </Text>
            )}
          </For>
        </View>
      </Bubble>
    )
  },
}

/** The block as a turn in the transcript. */
export function renderBlock(entry: Entry, ctx: BlockContext) {
  return (BLOCKS[entry.kind] as Renderer<Block["kind"]>)(entry, ctx)
}
