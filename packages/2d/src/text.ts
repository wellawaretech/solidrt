// Text runs in a sprite layer: a string drawn as one group of glyph
// sprites, one per glyph, from a sprite font's atlas. The engine shapes
// the text (kerning is its own), `layoutText` places the glyphs, and each
// becomes an ordinary sprite under the run's group - so camera, tint,
// `orderBy`, picking, transitions and bubbled pointer events already work,
// and a run moves as one handle. A glyph whose cell is not in the atlas
// yet is a hidden sprite at the right place; the font re-frames the run
// when the cell lands (or every run when the atlas grew).
//
// The node layer only: a run is a group. Three verbs like the sprites':
// addText, setText (re-laid out, the sprite pool reused), destroyText.
import { addGroup, addSprite, destroyGroup, destroySprite, setGroup, setSprite } from "./layer.ts"
import type { Sprite, SpriteGroup, SpriteLayer } from "./layer.ts"
import type { SpriteFont } from "./font.ts"
import { layoutText } from "./text-layout.ts"
import type { GlyphPlacement, TextAlign, TextAnchor, TextAnchorY, TextLayout } from "./text-layout.ts"

/** The run's own fields: what `addText` takes beyond the group's pose and
 * what `setText` changes (re-laying the run out). */
export type TextStyle = {
  text: string
  /** Pixels per em, layer pixels; default the font's `fontSize`. */
  fontSize?: number
  /** Baseline to baseline in layer pixels; default each line's own box. */
  lineHeight?: number
  /** Extra advance after every character, layer pixels (CSS letter-spacing); default the font's. */
  letterSpacing?: number
  /** Wrap at this width; unset, only `\n` breaks. */
  maxWidth?: number
  /** Line alignment inside the run's width (default "left"). */
  align?: TextAlign
  /** Where the run's x sits on its lines (default "start"). */
  anchor?: TextAnchor
  /** Where the run's y sits on its box (default "top"). */
  anchorY?: TextAnchorY
  /** The fill, RGBA 0..1 (default opaque white). */
  tint?: [number, number, number, number]
  /**
   * An outline under the glyphs (an msdf font only; a mask font ignores
   * it): its colour and width in layer pixels at the run's size. Null for
   * none.
   */
  outline?: { color: [number, number, number]; width: number } | null
  /** The draw-order key of every glyph (see SpriteOptions.renderOrder). */
  renderOrder?: number
}

/** The run's pose: the group's. */
export type TextPose = {
  x?: number
  y?: number
  /** Radians, clockwise, about the anchor point. */
  rotation?: number
  visible?: boolean
}

export type AddTextOptions = TextStyle &
  TextPose & {
    font: SpriteFont
    /** Mount under this group (null = the layer root). */
    parent?: SpriteGroup | null
  }

export type TextRun = {
  readonly layer: SpriteLayer
  readonly font: SpriteFont
  /** The run's group: its pose, and where glyph pointer events bubble. */
  readonly group: SpriteGroup
  /** The glyph sprites, in text order (one per glyph; a whitespace glyph
   * has none). */
  readonly sprites: readonly Sprite[]
  /** The run's box in layer pixels at the current style. */
  readonly width: number
  readonly height: number
  /** The first baseline below the box's top. */
  readonly ascent: number
  /** Line count. */
  readonly lines: number
  readonly text: string
  _style: TextStyle
  _layout: TextLayout
  /** Glyph ids with no cell yet: what a landing re-frames. */
  _pending: Set<number>
  _reframe(grew: boolean, arrived: ReadonlySet<number>): void
}

/** The internal mutable view (the readonly fields are the public face). */
type RunState = { -readonly [K in keyof TextRun]: TextRun[K] } & { sprites: Sprite[] }

/** Add a run to a node layer, drawn from `font`'s atlas, which the layer
 * must declare. Returns the run; its group carries the pose. */
export function addText(layer: SpriteLayer, opts: AddTextOptions): TextRun {
  let { font, parent, x, y, rotation, visible, ...style } = opts
  if (!layer.atlases.includes(font.atlas)) {
    throw new Error("addText: the layer does not declare the font's atlas; create it with the atlas in its list")
  }
  let group = addGroup(layer, { x, y, rotation, visible, parent })
  let run: RunState = {
    layer,
    font,
    group,
    sprites: [],
    width: 0,
    height: 0,
    ascent: 0,
    lines: 0,
    text: style.text,
    _style: style,
    _layout: { glyphs: [], lines: [], width: 0, height: 0, ascent: 0 },
    _pending: new Set(),
    _reframe(grew, arrived) {
      if (!grew && ![...run._pending].some(id => arrived.has(id))) return
      place(run)
    },
  }
  relayout(run, style)
  font._listeners.add(run)
  return run
}

/** Change a run: the style fields re-lay it out (the sprite pool is
 * reused), the pose fields move its group. Absent keys keep values. */
export function setText(run: TextRun, opts: Partial<TextStyle> & TextPose): void {
  let state = run as RunState
  if (state.group.layer === null) return
  let { x, y, rotation, visible, ...style } = opts
  if (x !== undefined || y !== undefined || rotation !== undefined || visible !== undefined) {
    setGroup(state.group, { x, y, rotation, visible })
  }
  let keys = Object.keys(style) as (keyof TextStyle)[]
  if (keys.some(k => style[k] !== undefined)) {
    let next: TextStyle = { ...state._style }
    for (let k of keys) if (style[k] !== undefined) (next as Record<string, unknown>)[k] = style[k]
    relayout(state, next)
  }
}

/** Remove the run: its group and every glyph sprite. */
export function destroyText(run: TextRun): void {
  let state = run as RunState
  state.font._listeners.delete(state)
  if (state.group.layer !== null) destroyGroup(state.group)
  state.sprites = []
}

// Shape, break and place: the layout from the style, then the sprites.
function relayout(run: RunState, style: TextStyle): void {
  checkStyle(style)
  run._style = style
  run.text = style.text
  let fontSize = style.fontSize ?? run.font.fontSize
  let prepared = run.font.prepare(style.text, { fontSize, lineHeight: 0, letterSpacing: style.letterSpacing })
  run._layout = layoutText(prepared, {
    maxWidth: style.maxWidth,
    align: style.align,
    anchor: style.anchor,
    anchorY: style.anchorY,
    lineHeight: style.lineHeight,
  })
  run.width = run._layout.width
  run.height = run._layout.height
  run.ascent = run._layout.ascent
  run.lines = run._layout.lines.length
  place(run)
}

// Write every glyph sprite from the layout and the font's cells: a cell
// the atlas lacks leaves its sprite hidden and asks the font for it.
function place(run: RunState): void {
  let { font, layer, group } = run
  let style = run._style
  let fontSize = style.fontSize ?? font.fontSize
  // Cells are made at `font.size` texels per em; the run draws at fontSize.
  let scale = fontSize / font.size
  let tint = style.tint ?? [1, 1, 1, 1]
  let outline = style.outline ?? null
  let renderOrder = style.renderOrder ?? 0
  let placements = run._layout.glyphs
  run._pending.clear()
  let missing: number[] = []
  // The pool: reuse sprites in order, add what is short, drop the rest.
  let sprites = run.sprites
  let used = 0
  for (let p of placements) {
    let glyph = font.glyph(p.id)
    if (glyph === null) {
      run._pending.add(p.id)
      missing.push(p.id)
    }
    // A glyph with no cell yet, or an empty one (a space), draws nothing:
    // its sprite stays hidden at a point, so a landing cell re-frames in
    // place.
    let drawn = glyph !== null && glyph.width > 0 && glyph.height > 0
    let w = drawn ? glyph!.width * scale : 0
    let h = drawn ? glyph!.height * scale : 0
    let fields = {
      x: p.x + (drawn ? glyph!.left * scale + w / 2 : 0),
      y: p.y + (drawn ? -glyph!.top * scale + h / 2 : 0),
      w,
      h,
      frame: drawn ? glyph!.frame : undefined,
      tint,
      renderOrder,
      visible: drawn,
    }
    let sprite = sprites[used]
    if (sprite === undefined) {
      sprite = addSprite(layer, { ...fields, parent: group })
      sprites.push(sprite)
    } else {
      setSprite(sprite, fields)
    }
    if (outline !== null) layer._outline(sprite, outline.color[0], outline.color[1], outline.color[2], outline.width)
    else layer._outline(sprite, 0, 0, 0, 0)
    used++
  }
  for (let i = used; i < sprites.length; i++) destroySprite(sprites[i]!)
  sprites.length = used
  if (missing.length > 0) font.request(missing)
}

function checkStyle(style: TextStyle): void {
  if (typeof style.text !== "string") throw new Error(`text: text must be a string, got ${typeof style.text}`)
  if (style.fontSize !== undefined && !(style.fontSize > 0 && Number.isFinite(style.fontSize))) {
    throw new Error(`text: fontSize must be a positive number, got ${style.fontSize}`)
  }
  if (style.outline != null) {
    let { color, width } = style.outline
    if (!(Array.isArray(color) && color.length === 3 && color.every(c => c >= 0 && c <= 1))) {
      throw new Error(`text: outline.color must be [r, g, b] in 0..1, got ${JSON.stringify(color)}`)
    }
    if (!(width >= 0 && Number.isFinite(width))) throw new Error(`text: outline.width must be >= 0, got ${width}`)
  }
  if (style.tint !== undefined && !(style.tint.length === 4 && style.tint.every(c => c >= 0 && c <= 1))) {
    throw new Error(`text: tint must be [r, g, b, a] in 0..1, got ${JSON.stringify(style.tint)}`)
  }
}

export type { GlyphPlacement }
