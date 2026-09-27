import { Show, parseColor, withAlpha } from "@solidrt/core"
import type { Color, Gradient, ShadowProps, TransitionEndEvent, TransitionProps as CoreTransitionProps } from "@solidrt/core"
import { theme } from "./theme"
import { policy } from "./policy"
import { bevelStroke, contactShadow, fadedShadow, keyShadow, sheenFill, themeAxis } from "./light"
import { glowShadow } from "./glow"
import { colorFade, elevationMotion, feedbackFade } from "./motion"
import type { ElevationLevel, Glow, Material, MaterialRole } from "./types"
import { withTransitionDefaults } from "./types"

// The one way a component draws a face. A face is a role's material under
// the theme light at an elevation, and Surface stacks the layers that make
// it, in this order:
//
//   contact shadow   tight occlusion where the face meets the ground; stays
//                    when a press fades the key shadow out
//   glow             the material's emitted light, a zero-offset halo
//   fill             the base color, tinted by the light and with the sheen
//                    gradient along it when the material has one; carries
//                    the key shadow, which the elevation casts along the
//                    light, drops to a tenth while pressed and retargets to
//                    `liftTo` while lifted (all on the native shadow
//                    transition); under a glass material the fill sits in
//                    a backdrop-filtered view and the key shadow moves to a
//                    caster of its own
//   tint             the hover/pressed overlay (PressFeedback), only when
//                    the face is pressable (hovered or pressed given)
//   bevel            a one-pixel stroke from the lit edge to the shaded one
//   outline          the border or the focus ring the component decides on
//
// Everything is detached and fills the view Surface is rendered in, so it
// adds nothing to layout, except the glass view, which is absolutely
// positioned over the same box: render Surface as the FIRST child of the
// laid-out view it dresses. Under the stock presets (flat materials, zero
// elevation) a face is a plain fill, the tint and the outline: the look the
// package has always had.
//
// Shadows cast from opaque casters. The contact shadow and the glow sit on
// rects in the fill's own color, hidden under the fill; the key shadow on
// the fill rect itself. A transparent caster's blurred shadow composites in
// a save layer of its own, which a tiled GPU pays for per shadow - a list of
// such rows ran at 8-19 fps on every measured device
// (okf/notes/lit-list-cost.md) - so only a see-through face (glass, a
// translucent fill) casts from a transparent rect, and those are the few
// overlays.

export interface SurfaceProps {
  role: MaterialRole
  // The level the face stands at; default flat. The theme maps it to a
  // height (theme.elevation), zero under the stock presets.
  elevation?: ElevationLevel
  // The base color: what the light tints and the sheen runs over, and the
  // color a label's polarity is judged against. Defaults per role: surface
  // and overlay to color.surface, control to surfaceAlt, accent to primary.
  fill?: Color | Gradient
  radius?: number | [number, number, number, number]
  // Layers merged over the role's material (a component's theme override
  // and instance style).
  material?: Partial<Material>
  // The glow layer alone: set replaces the material's, null clears it.
  glow?: Glow | null
  // A pressed face sinks: the key shadow fades toward the contact shadow and
  // the lit edge moves to the far side. Giving `pressed` or `hovered` also
  // mounts the tint layer, unless `tint` is false (a knob that sinks under
  // the finger but takes no hover tint); `active` widens the glow on its
  // own, for a face that neither tints nor sinks (a track under a press).
  pressed?: boolean
  hovered?: boolean
  tint?: boolean
  active?: boolean
  // Hover lift: while `lifted`, the key shadow retargets to the one cast
  // from `liftTo`. Without `liftTo` nothing lifts.
  lifted?: boolean
  liftTo?: ElevationLevel
  // A well or a groove (a track, a field): shaded like a pressed face and
  // casting nothing.
  sunken?: boolean
  // False hides fill, sheen, bevel, shadows and glow, keeping only the tint
  // (a ghost button at rest). Default true.
  chrome?: boolean
  // The border or the focus ring, as the component resolves it; null or
  // undefined draws none.
  outline?: { color: Color | Gradient; width: number } | null
  // The component's transition entries for its faces: `fillTransition` is
  // its `background` entry (backgroundColor, borderRadius), `outlineTransition`
  // its `border` entry, each already split from the component's `transition`
  // prop; the fades and the elevation motion are filled in as defaults.
  fillTransition?: CoreTransitionProps["transition"]
  outlineTransition?: CoreTransitionProps["transition"]
  onFillTransitionEnd?: (event: TransitionEndEvent) => void
  onOutlineTransitionEnd?: (event: TransitionEndEvent) => void
}

// Key shadow strength while pressed: nearly gone, so the contact shadow is
// what remains of the face's lift.
const PRESSED_KEY_SHADOW = 0.1
// The saturation a glass face gives what shows through it: a little more
// than life, so the frosted backdrop reads as glass and not as fog.
const GLASS_SATURATE = 1.4
// The alpha of a glass tint that takes the face's own fill.
const GLASS_ALPHA = 0.6
// The bevel's stroke width: one logical pixel, an edge and not a band.
const BEVEL_WIDTH = 1

let isOpaque = (c: Color | Gradient) => typeof c !== "string" || ((parseColor(c) >>> 0) & 0xff) === 0xff

/** The fill a role's face takes when the component gives none. */
export function roleFill(role: MaterialRole): string {
  switch (role) {
    case "control":
      return theme.color.surfaceAlt
    case "accent":
      return theme.color.primary
    default:
      return theme.color.surface
  }
}

/** The role's material with the component's layers merged over it. */
export function roleMaterial(role: MaterialRole, override?: Partial<Material>): Material {
  let m = theme.material[role]
  return override ? { ...m, ...override } : m
}

/**
 * Whether a face at this level gives a press something to sink: a sheen, a
 * bevel or a height. A flat face keeps the press scale instead (see
 * pressScale), since it has no depth to lose.
 */
export function surfaceSinks(role: MaterialRole, elevation: ElevationLevel = "flat", material?: Partial<Material>): boolean {
  let m = roleMaterial(role, material)
  return (m.sheen ?? 0) > 0 || (m.bevel ?? 0) > 0 || theme.elevation[elevation] > 0
}

/** The paint of a face, for a part a component draws itself (a slider fill, a segmented indicator). */
export type FacePaint = {
  fill: Color | Gradient
  keyShadow?: ShadowProps
  contactShadow?: ShadowProps
  glow?: ShadowProps
  bevel?: Gradient
}

/**
 * Derives a face's paint from the theme light and the role's material, the
 * same derivation Surface draws with. `active` widens the glow (hovered or
 * pressed); `pressed` and `sunken` invert the sheen and the bevel and, for a
 * press, fade the key shadow. Reads the theme reactively: call it inside the
 * prop expressions that use it.
 */
export function facePaint(o: {
  role: MaterialRole
  fill?: Color | Gradient
  elevation?: ElevationLevel
  material?: Partial<Material>
  glow?: Glow | null
  pressed?: boolean
  sunken?: boolean
  active?: boolean
}): FacePaint {
  let light = theme.light
  let axis = themeAxis()
  let material = roleMaterial(o.role, o.material)
  let base = o.fill ?? roleFill(o.role)
  let invert = o.pressed === true || o.sunken === true
  let height = o.sunken ? 0 : theme.elevation[o.elevation ?? "flat"]
  let key = keyShadow(light, axis, height, theme.shadow)
  return {
    fill: sheenFill(base, light, axis, material.sheen, invert),
    keyShadow: key && o.pressed ? fadedShadow(key, PRESSED_KEY_SHADOW) : key,
    contactShadow: contactShadow(light, axis, height, theme.shadow),
    glow: glowShadow(o.glow !== undefined ? o.glow : material.glow, base, o.active),
    bevel: bevelStroke(light, axis, material.bevel, invert),
  }
}

export function Surface(props: SurfaceProps) {
  let chrome = () => props.chrome !== false
  let base = () => props.fill ?? roleFill(props.role)
  let material = () => roleMaterial(props.role, props.material)
  let glass = () => (chrome() ? material().glass : undefined)
  let paint = () =>
    facePaint({
      role: props.role,
      fill: base(),
      elevation: props.elevation,
      material: props.material,
      glow: props.glow,
      pressed: props.pressed,
      sunken: props.sunken,
      active: props.active ?? (props.pressed || props.hovered),
    })
  // The key shadow the face casts now: while lifted, the one from the lift
  // level; the retarget is what the shadow transition animates.
  let key = (): ShadowProps | undefined => {
    if (!chrome()) return undefined
    if (props.lifted && props.liftTo && !props.sunken) {
      let light = theme.light
      return keyShadow(light, themeAxis(), theme.elevation[props.liftTo], theme.shadow)
    }
    return paint().keyShadow
  }
  let contact = () => (chrome() ? paint().contactShadow : undefined)
  let glow = () => (chrome() ? paint().glow : undefined)
  let fill = () => (chrome() ? paint().fill : "transparent")
  let bevel = () => (chrome() ? paint().bevel : undefined)
  // Opaque faces cast from casters in their own color, hidden under the
  // fill; see-through ones from transparent casters (their own layer each).
  let caster = () => (glass() || !isOpaque(base()) ? "transparent" : base())
  let keyOnFill = () => !glass() && caster() !== "transparent"
  let glassFill = () => {
    let g = glass()!
    let tint = g.tint ?? (typeof base() === "string" ? withAlpha(base() as string, GLASS_ALPHA) : base())
    return sheenFill(tint, theme.light, themeAxis(), material().sheen, props.pressed === true || props.sunken === true)
  }

  let fillTransition = () => withTransitionDefaults(props.fillTransition, { ...colorFade(), ...elevationMotion() })
  let shadowTransition = () => elevationMotion()
  let outlineTransition = () => withTransitionDefaults(props.outlineTransition, colorFade())
  let tinted = () => props.tint !== false && (props.pressed !== undefined || props.hovered !== undefined)
  let tint = () => (props.pressed ? theme.color.overlayPressed : theme.color.overlayHover)
  let tintFade = () => {
    let f = feedbackFade()
    return f === undefined ? undefined : { color: f }
  }

  return (
    <>
      <Show when={contact()}>
        {(shadow) => <d-rect color={caster()} radius={props.radius} shadow={shadow()} transition={shadowTransition()} />}
      </Show>
      <Show when={glow()}>{(shadow) => <d-rect color={caster()} radius={props.radius} shadow={shadow()} />}</Show>
      <Show when={!keyOnFill() && key()}>
        {(shadow) => <d-rect color="transparent" radius={props.radius} shadow={shadow()} transition={shadowTransition()} />}
      </Show>
      <Show
        when={glass()}
        fallback={
          <d-rect
            transition={fillTransition()}
            onTransitionEnd={props.onFillTransitionEnd}
            color={fill()}
            radius={props.radius}
            shadow={keyOnFill() ? key() : undefined}
          />
        }
      >
        {(g) => (
          <view
            position="absolute"
            top={0}
            left={0}
            right={0}
            bottom={0}
            overflow="hidden"
            clipRadius={props.radius}
            backdropFilter={{ blur: g().blur, saturate: GLASS_SATURATE }}
          >
            <d-rect transition={fillTransition()} onTransitionEnd={props.onFillTransitionEnd} color={glassFill()} radius={props.radius} />
          </view>
        )}
      </Show>
      <Show when={tinted()}>
        <d-rect
          transition={tintFade()}
          color={props.pressed || props.hovered ? tint() : withAlpha(tint(), 0)}
          radius={props.radius}
        />
      </Show>
      <Show when={bevel()}>
        {(b) => <d-rect drawStyle="stroke" strokeWidth={BEVEL_WIDTH} color={b()} radius={props.radius} />}
      </Show>
      <Show when={props.outline}>
        {(o) => (
          <d-rect
            drawStyle="stroke"
            transition={outlineTransition()}
            onTransitionEnd={props.onOutlineTransitionEnd}
            color={o().color}
            strokeWidth={o().width}
            radius={props.radius}
          />
        )}
      </Show>
    </>
  )
}
