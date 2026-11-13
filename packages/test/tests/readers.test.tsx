// The verbs an app test has beside naming nodes and sending pointer input:
// a link in and the links, shares and haptics out, the clipboard both ways, a debug
// command, a synthetic gamepad, and the readers past a node's record: the
// subtree as text, its pixels, the GPU inventory.

import { registerDebug } from "sol:dev"
import { file } from "flux:fs"
import { decodeImage, encodeImage } from "flux:image"
import { test, expect } from "../src/index.ts"
import { createSignal, gamepads, onLink, openLink, touchFeedback } from "@solidrt/core"

// The error the promise `run` returns rejects with; fails when it
// fulfills, and when `run` throws instead of returning a promise (a call
// that must reject never throws).
async function rejection(run: () => unknown): Promise<Error> {
  let pending = run()
  if (!(pending instanceof Promise)) throw new Error(`Expected the call to return a promise, got ${String(pending)}`)
  try {
    await pending
  } catch (err) {
    return err as Error
  }
  throw new Error("Expected the call to fail, and it passed")
}

// The message `run` rejects with; fails when it fulfills.
async function failure(run: () => Promise<unknown>): Promise<string> {
  return String((await rejection(run)).message)
}

// The clipboard images: a square of SIDE pixels, rows top-down, each an
// opaque RGB triple (red, green / blue, white).
const SIDE = 2
const PIXELS = [
  [255, 0, 0],
  [0, 255, 0],
  [0, 0, 255],
  [255, 255, 255],
]
const OPAQUE = 255
// The eight bytes every PNG file starts with.
const PNG_SIGNATURE = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]
// An uncompressed 24-bit BMP: the file header, the BITMAPINFOHEADER, then
// the rows bottom-up, BGR, each padded to ROW_ALIGN bytes. Every header
// field is little-endian.
const BMP_FILE_HEADER_BYTES = 14
const BMP_INFO_HEADER_BYTES = 40
const BMP_PLANES = 1
const BMP_BITS_PER_PIXEL = 24
const BMP_BYTES_PER_PIXEL = 3
const BMP_ROW_ALIGN = 4
// The file header: the "BM" magic, the file size, the pixel data offset.
const BMP_MAGIC = [0x42, 0x4d]
const BMP_FILE_SIZE_AT = 2
const BMP_PIXEL_OFFSET_AT = 10
// The BITMAPINFOHEADER: its own size, the width, the height (positive:
// rows bottom-up), the planes, the bits per pixel, the pixel data size.
const BMP_INFO_SIZE_AT = 14
const BMP_WIDTH_AT = 18
const BMP_HEIGHT_AT = 22
const BMP_PLANES_AT = 26
const BMP_BITS_AT = 28
const BMP_IMAGE_SIZE_AT = 34
// A length no array index reaches, past the int32 range a length usually
// has: what a script can pass to make an engine that assumes int32 abort.
const HUGE_LENGTH = 2 ** 31

/** PIXELS as RGBA8, the shape `decodeImage` returns. */
function rgba(): Uint8Array {
  return new Uint8Array(PIXELS.flatMap(([r, g, b]) => [r!, g!, b!, OPAQUE]))
}

/** PIXELS as a PNG file. */
function png(): Uint8Array {
  return encodeImage({ width: SIDE, height: SIDE, data: rgba() }, { alpha: "straight" })
}

/** PIXELS as a BMP file, what a Windows bitmap reads as. */
function bmp(): Uint8Array {
  let stride = Math.ceil((SIDE * BMP_BYTES_PER_PIXEL) / BMP_ROW_ALIGN) * BMP_ROW_ALIGN
  let offset = BMP_FILE_HEADER_BYTES + BMP_INFO_HEADER_BYTES
  let bytes = new Uint8Array(offset + stride * SIDE)
  let view = new DataView(bytes.buffer)
  bytes.set(BMP_MAGIC)
  view.setUint32(BMP_FILE_SIZE_AT, bytes.length, true)
  view.setUint32(BMP_PIXEL_OFFSET_AT, offset, true)
  view.setUint32(BMP_INFO_SIZE_AT, BMP_INFO_HEADER_BYTES, true)
  view.setInt32(BMP_WIDTH_AT, SIDE, true)
  view.setInt32(BMP_HEIGHT_AT, SIDE, true)
  view.setUint16(BMP_PLANES_AT, BMP_PLANES, true)
  view.setUint16(BMP_BITS_AT, BMP_BITS_PER_PIXEL, true)
  view.setUint32(BMP_IMAGE_SIZE_AT, stride * SIDE, true)
  for (let row = 0; row < SIDE; row++) {
    let start = offset + (SIDE - 1 - row) * stride
    for (let column = 0; column < SIDE; column++) {
      let [r, g, b] = PIXELS[row * SIDE + column]!
      bytes.set([b!, g!, r!], start + column * BMP_BYTES_PER_PIXEL)
    }
  }
  return bytes
}

test("a link reaches onLink, and an app without a handler fails the verb", async app => {
  let message = await failure(() => app.link("app://early"))
  expect(message).toContain("does not listen for links")
  let [screen, setScreen] = createSignal("home")
  onLink(link => setScreen(link))
  await app.mount(() => <text>{screen()}</text>)
  await app.link("app://settings/display")
  expect(app.find({ text: "app://settings/display" }).exists).toBe(true)
})

test("a link the app opens is recorded, not opened", async app => {
  await app.mount(() => <text>credits</text>)
  expect(app.requestedLinks()).toEqual([])
  await openLink("https://example.com/credits")
  await openLink("mailto:feedback@example.com")
  expect(app.requestedLinks()).toEqual(["https://example.com/credits", "mailto:feedback@example.com"])
})

test("a share the app asks for is recorded, no sheet shown", async app => {
  await app.mount(() => <text>board</text>)
  expect(app.requestedShares()).toEqual([])
  await navigator.share!({ text: "My board", files: [file("share/board.png")] })
  expect(app.requestedShares()).toEqual([{ title: null, text: "My board", url: null, files: [{ path: "share/board.png", name: "board.png" }] }])
})

test("haptic feedback the app asks for is recorded, nothing played", async app => {
  await app.mount(() => <text>switch</text>)
  expect(app.requestedHaptics()).toEqual([])
  touchFeedback("toggleOn")
  touchFeedback("impact", { weight: "heavy", intensity: 0.5 })
  let requested = app.requestedHaptics()
  expect(requested).toMatchObject([
    { intent: "toggleOn", weight: null, intensity: null },
    { intent: "impact", weight: "heavy", intensity: 0.5 },
  ])
  for (let call of requested) {
    expect(Number.isFinite(call.at)).toBe(true)
    expect(call.at).toBeGreaterThanOrEqual(0)
  }
})

test("the image clipboard is listed under test, whatever the platform", async app => {
  await app.mount(() => <text>copy</text>)
  expect(Flux.capabilities).toContain("clipboard-image")
})

test("text the app writes is the clipboard's, and reads back", async app => {
  await app.mount(() => <text>copy</text>)
  expect(app.clipboard()).toBeNull()
  expect(await navigator.clipboard.readText()).toBe("")
  await navigator.clipboard.writeText("https://example.com/invite")
  expect(app.clipboard()).toEqual({ text: "https://example.com/invite" })
  expect(await navigator.clipboard.readText()).toBe("https://example.com/invite")
  await navigator.clipboard.writeText("")
  expect(app.clipboard()).toBeNull()
  let error = await rejection(() => navigator.clipboard.writeText(7 as unknown as string))
  expect(error).toBeInstanceOf(TypeError)
})

test("writeText rejects, never throws, for no text and for a lone surrogate", async app => {
  await app.mount(() => <text>copy</text>)
  let write = navigator.clipboard.writeText as (text?: string) => Promise<void>
  expect(await rejection(() => write())).toBeInstanceOf(TypeError)
  expect(await rejection(() => write("\ud800"))).toBeInstanceOf(TypeError)
  expect(app.clipboard()).toBeNull()
})

test("an image the app writes is the clipboard's, and replaces the text", async app => {
  await app.mount(() => <text>copy</text>)
  await navigator.clipboard.writeText("before")
  let bytes = png()
  await navigator.clipboard.write([new ClipboardItem({ "image/png": bytes })])
  let held = app.clipboard() as { type: string; data: Uint8Array }
  expect(held.type).toBe("image/png")
  expect(Array.from(held.data)).toEqual(Array.from(bytes))
  expect(await navigator.clipboard.readText()).toBe("")
})

test("an image a test puts is what the app reads", async app => {
  await app.mount(() => <text>paste</text>)
  app.setClipboard({ type: "image/png", data: png() })
  let items = await navigator.clipboard.read()
  expect(items).toHaveLength(1)
  expect(items[0]!.types).toEqual(["image/png"])
  let image = decodeImage(await items[0]!.getType("image/png"))
  expect([image.width, image.height]).toEqual([SIDE, SIDE])
  expect(Array.from(image.data)).toEqual(Array.from(rgba()))
})

test("a bitmap a test puts is read as a PNG with its pixels", async app => {
  await app.mount(() => <text>paste</text>)
  app.setClipboard({ type: "image/bmp", data: bmp() })
  let items = await navigator.clipboard.read()
  expect(items).toHaveLength(1)
  expect(items[0]!.types).toEqual(["image/png"])
  let bytes = await items[0]!.getType("image/png")
  expect(Array.from(bytes.subarray(0, PNG_SIGNATURE.length))).toEqual(PNG_SIGNATURE)
  let image = decodeImage(bytes)
  expect([image.width, image.height]).toEqual([SIDE, SIDE])
  expect(Array.from(image.data)).toEqual(Array.from(rgba()))
})

test("read resolves no item for an empty clipboard or a text", async app => {
  await app.mount(() => <text>paste</text>)
  expect(await navigator.clipboard.read()).toEqual([])
  app.setClipboard({ text: "pasted" })
  expect(await navigator.clipboard.read()).toEqual([])
  expect(await navigator.clipboard.readText()).toBe("pasted")
  app.setClipboard(null)
  expect(await navigator.clipboard.readText()).toBe("")
})

test("write rejects with a TypeError for anything but one PNG item", async app => {
  await app.mount(() => <text>copy</text>)
  let item = new ClipboardItem({ "image/png": png() })
  let calls: [() => Promise<void>, string][] = [
    [() => navigator.clipboard.write([]), "got 0"],
    [() => navigator.clipboard.write([item, item]), "got 2"],
    [() => navigator.clipboard.write([new ClipboardItem({ "image/jpeg": png() })]), "image/jpeg"],
    [() => navigator.clipboard.write([new ClipboardItem({ "image/png": new Uint8Array([1, 2, 3]) })]), "not a PNG"],
  ]
  for (let [call, message] of calls) {
    let error = await rejection(call)
    expect(error).toBeInstanceOf(TypeError)
    expect(error.message).toContain(message)
  }
  expect(app.clipboard()).toBeNull()
})

test("write rejects, never throws, for an argument it cannot read", async app => {
  await app.mount(() => <text>copy</text>)
  let write = navigator.clipboard.write as (items?: unknown) => Promise<void>
  let throwing = [new ClipboardItem({ "image/png": png() })]
  Object.defineProperty(throwing, 0, {
    get() {
      throw new Error("no item here")
    },
  })
  let calls: [() => Promise<void>, string][] = [
    [() => write(), "pass an array"],
    [() => write("x"), "pass an array"],
    [() => write([{ "image/png": png() }]), "must be a ClipboardItem"],
    [() => write(new Array(HUGE_LENGTH)), `got ${HUGE_LENGTH}`],
    [() => write(throwing), "no item here"],
  ]
  for (let [call, message] of calls) {
    let error = await rejection(call)
    expect(error).toBeInstanceOf(TypeError)
    expect(error.message).toContain(message)
  }
  // A later valid write still succeeds after the getter threw.
  await navigator.clipboard.write([new ClipboardItem({ "image/png": png() })])
  expect((app.clipboard() as { type: string }).type).toBe("image/png")
})

test("a ClipboardItem holds Uint8Arrays only, and a type it holds", async app => {
  await app.mount(() => <text>copy</text>)
  expect(() => new ClipboardItem({ "image/png": "x" as unknown as Uint8Array })).toThrow(TypeError)
  expect(() => new ClipboardItem({})).toThrow(TypeError)
  expect(() => new ClipboardItem(5 as unknown as Record<string, Uint8Array>)).toThrow(TypeError)
  let construct = ClipboardItem as unknown as new (record?: unknown) => ClipboardItem
  expect(() => new construct()).toThrow(TypeError)
  expect(() => new construct()).toThrow("new ClipboardItem:")
  let item = new ClipboardItem({ "image/png": png() })
  let error = await rejection(() => item.getType("text/plain"))
  expect(error.name).toBe("NotFoundError")
  let getType = item.getType.bind(item) as (type?: unknown) => Promise<Uint8Array>
  expect(await rejection(() => getType(42))).toBeInstanceOf(TypeError)
  expect(await rejection(() => getType())).toBeInstanceOf(TypeError)
})

test("a ClipboardItem's types are its keys as given, and must be valid text", async app => {
  await app.mount(() => <text>copy</text>)
  let first = new Uint8Array([1])
  let second = new Uint8Array([2])
  let item = new ClipboardItem({ a: first, "a\0b": second })
  expect(item.types).toEqual(["a", "a\0b"])
  expect(Array.from(await item.getType("a"))).toEqual([1])
  expect(Array.from(await item.getType("a\0b"))).toEqual([2])
  expect(() => new ClipboardItem({ "\ud800": first })).toThrow(TypeError)
})

test("a ClipboardItem is a snapshot, and getType hands out copies", async app => {
  await app.mount(() => <text>copy</text>)
  let source = png()
  let original = Array.from(source)
  let item = new ClipboardItem({ "image/png": source })
  expect(Object.isFrozen(item.types)).toBe(true)
  source.fill(0)
  let first = await item.getType("image/png")
  expect(Array.from(first)).toEqual(original)
  first.fill(0)
  expect(Array.from(await item.getType("image/png"))).toEqual(original)
})

test("a debug command is called with its args and returns its value", async app => {
  let [level, setLevel] = createSignal(1)
  registerDebug("level", (next: number) => {
    setLevel(next)
    return { was: 1, now: next }
  })
  await app.mount(() => <text>{`level ${level()}`}</text>)
  expect(await app.debug("level", 7)).toEqual({ was: 1, now: 7 })
  expect(app.find({ text: "level 7" }).exists).toBe(true)
  expect(await failure(() => app.debug("warp"))).toContain("no debug command 'warp' (registered: level)")
})

test("a debug command's unregister removes it, and only its own registration", async app => {
  let off = registerDebug("probe", () => 1)
  registerDebug("keep", () => 2)
  await app.mount(() => <text>probe</text>)
  expect(await app.debug("probe")).toBe(1)
  off()
  expect(await failure(() => app.debug("probe"))).toContain("no debug command 'probe' (registered: keep)")
  // A stale unregister: the name has been registered again since.
  let stale = registerDebug("again", () => 1)
  registerDebug("again", () => 2)
  stale()
  expect(await app.debug("again")).toBe(2)
})

test("a synthetic gamepad connects, holds a state and leaves", async app => {
  await app.mount(() => <text>{gamepads().map(pad => (pad ? pad.buttons.join("+") || "idle" : "none")).join(",") || "no pads"}</text>)
  expect(app.find({ text: "no pads" }).exists).toBe(true)
  await app.input([{ type: "gamepad", action: "connect" }])
  expect(app.find({ text: "idle" }).exists).toBe(true)
  await app.input([{ type: "gamepad", action: "set", slot: 0, buttons: ["south", "dpadUp"] }])
  expect(app.find({ text: "south+dpadUp" }).exists).toBe(true)
  await app.input([{ type: "gamepad", action: "set", slot: 0, buttons: ["east"], holdMs: 100 }])
  expect(app.find({ text: "idle" }).exists).toBe(true)
  await app.input([{ type: "gamepad", action: "disconnect", slot: 0 }])
  expect(app.find({ text: "none" }).exists).toBe(true)
  expect(await failure(() => app.input([{ type: "gamepad", action: "set", slot: 3 }]))).toContain("slot 3 holds no synthetic pad")
})

test("every test starts with no pad seated", async app => {
  await app.mount(() => <text>{String(gamepads().length)}</text>)
  await app.frame()
  expect(app.find({ text: "0" }).exists).toBe(true)
})

test("outline() is the subtree as text, without ids", async app => {
  let root = await app.mount(() => (
    <view label="card" width={200} height={80} flexDirection="column" opacity={0.5}>
      <text>Title</text>
      <view label="bar" width={50} height={10} />
    </view>
  ))
  let card = root.find({ label: "card" })
  let lines = card.outline().split("\n")
  expect(lines[0]).toBe("view [card] 0,0 200x80")
  expect(/^  text "Title" 0,0 \d+(\.\d+)?x\d+(\.\d+)?$/.test(lines[1]!)).toBe(true)
  expect(/^  view \[bar\] 0,\d+(\.\d+)? 50x10$/.test(lines[2]!)).toBe(true)
  expect(lines.length).toBe(3)
  expect(card.outline({ props: true }).split("\n")[0]).toContain('"opacity":0.5')
})

test("pixel reads what a node paints, where a transition stands", async app => {
  let [on, setOn] = createSignal(false)
  let swatch = app.ref()
  await app.mount(() => (
    <view width={200} height={100}>
      <rect width={200} height={100} color="#000000" />
      <rect ref={swatch} position="absolute" left={0} top={0} width={40} height={40} color={on() ? "#000000" : "#ff0000"} transition="100ms linear" />
    </view>
  ))
  expect(swatch.pixel(20, 20)).toEqual([255, 0, 0, 255])
  let image = swatch.pixels()
  expect([image.width, image.height, image.data.length]).toEqual([40, 40, 40 * 40 * 4])
  let before = app.time
  setOn(true)
  await app.advance(50)
  let half = swatch.pixel(20, 20)
  expect(half[0]).toBeGreaterThan(40)
  expect(half[0]).toBeLessThan(220)
  // Reading drew the tree as it stood and passed no time.
  expect(app.time - before).toBeLessThan(70)
  expect(() => swatch.pixel(40, 0)).toThrow("outside")
})

test("gpu is the inventory the control API reports", async app => {
  await app.mount(() => <text>plain</text>)
  let inventory = app.gpu()
  expect(Object.keys(inventory).sort()).toEqual(["buffers", "forceFullRenders", "groups", "pipelines", "programs", "renderPipelines", "textures"])
  // The damage debug switch beside the inventory, off at a test's start.
  expect(inventory.forceFullRenders).toBe(false)
  expect(app.gpu({ label: "no such label" }).textures).toEqual([])
})
