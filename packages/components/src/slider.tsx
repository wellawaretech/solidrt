import { arena, createMemo, createSignal, focusedNode, getBoundingBox, onLayout, onSettled } from "@solidrt/core"
import type { KeyEvent, LayoutProps, PointerEvent } from "@solidrt/core"
import { pill, theme } from "./theme"
import { policy } from "./policy"
import { densityScale } from "./density"
import { Surface, facePaint } from "./surface"
import type { StyleProps, TransitionProps } from "./types"
import { splitTransition, transitionEndFor } from "./types"
import { colorFade } from "./motion"

export interface SliderProps extends TransitionProps {
  // Controlled value. If omitted, the slider is uncontrolled.
  value?: number
  defaultValue?: number
  min?: number
  max?: number
  // Snap increment. Omit for continuous.
  step?: number
  onChange?: (value: number) => void
  disabled?: boolean
  layout?: LayoutProps
  style?: StyleProps
}

const HEIGHT = 24
const GROOVE = 4
const THUMB = 20

let clamp = (x: number, lo: number, hi: number) => (x < lo ? lo : x > hi ? hi : x)

// A horizontal slider. The groove fills up to the thumb; dragging or pressing
// the track sets the value from the pointer x. Moves arrive on the frozen down
// path, so a drag keeps updating when the pointer drifts off the track.
// Focused (spatial nav), arrow keys step the value (by `step`, else 1% of the
// range) and the thumb draws the focus ring under the focusRing policy.
// Controlled via value/onChange, or uncontrolled via defaultValue. The
// groove is a sunken control face (surfaceAlt), the fill an accent face
// (primary, with the accent material's glow) drawn as a detached rect so a
// drag never reflows, the thumb a raised control face (theme.color.thumb,
// else primary; style.elevation sets its level) that sinks while dragged;
// groove and thumb take pill(), round by default and square under
// radius.full 0.
export function Slider(props: SliderProps) {
  let min = () => props.min ?? 0
  let max = () => props.max ?? 100
  let [internal, setInternal] = createSignal(props.defaultValue ?? props.min ?? 0)
  let value = () => (props.value !== undefined ? props.value : internal())

  let track: { id: number } | undefined
  let active: number | null = null
  let [dragging, setDragging] = createSignal(false)

  let height = () => Math.round(HEIGHT * densityScale())
  let thumb = () => Math.round(THUMB * densityScale())

  // Theme-level per-component overrides merged under the instance style:
  // backgroundColor is the groove, color the fill, borderRadius the groove's
  // (ProgressBar's reading of the same keys), and the border boxes the groove.
  let styled = () => ({ ...theme.components.slider, ...props.style })
  let grooveColor = () => styled().backgroundColor ?? theme.color.surfaceAlt
  let fillColor = () => styled().color ?? theme.color.primary
  let grooveRadius = () => styled().borderRadius ?? pill(GROOVE)
  let hasBorder = () => styled().borderWidth != null || styled().borderColor != null
  let fillPaint = () => facePaint({ role: "accent", fill: fillColor(), material: styled().material, glow: props.disabled ? null : styled().glow })
  let split = () => splitTransition(props.transition)

  let pct = () => clamp(((value() - min()) / (max() - min())) * 100, 0, 100)

  // Measured groove width in pixels. The fill and thumb are driven off this
  // rather than a percentage `width`/`left`, so dragging repaints (d-rect `w`,
  // thumb `x` transform) instead of reflowing taffy every move. Refreshed each
  // layout so it tracks resizes.
  let groove: { id: number } | undefined
  let [grooveWidth, setGrooveWidth] = createSignal(0)
  onLayout(() => {
    if (groove) setGrooveWidth(getBoundingBox(groove)?.width ?? 0)
  })
  let fillPx = () => (pct() / 100) * grooveWidth()

  let commit = (v: number) => {
    if (props.value === undefined) setInternal(v)
    props.onChange?.(v)
  }

  // The handlers sit on the track, and localX is exact in the track's own
  // frame even after a drag drifts off it (frozen down path), so no origin
  // lookup is needed. clientX against getBoundingBox would mix frames: the
  // box's x/y are relative to the nearest positioned ancestor, not the
  // window clientX lives in.
  let setFromLocalX = (localX: number) => {
    if (!track) return
    let width = getBoundingBox(track)?.width ?? 0
    if (width === 0) return
    let f = clamp(localX / width, 0, 1)
    let raw = min() + f * (max() - min())
    if (props.step) raw = Math.round(raw / props.step) * props.step
    commit(clamp(raw, min(), max()))
  }

  let endDrag = () => {
    if (active != null) {
      arena.release(active, owner)
      active = null
    }
    setDragging(false)
  }
  let owner = { cancel: endDrag }

  // An unmount mid-drag must not leave a resolved claim behind.
  onSettled(() => endDrag)

  let handleDown = (e: PointerEvent) => {
    if (props.disabled || active != null) return
    // A down on the track is unambiguously a slider drag: resolve the arena
    // outright so an ancestor scroller's pan cannot take the pointer over.
    arena.steal(e.pointerId, owner)
    active = e.pointerId
    setDragging(true)
    setFromLocalX(e.localX)
  }
  let handleMove = (e: PointerEvent) => {
    if (active !== e.pointerId) return
    setFromLocalX(e.localX)
  }
  let handleUp = (e: PointerEvent) => {
    if (active === e.pointerId) endDrag()
  }

  // focusedNode() is read first, unconditionally, so the memo depends on it
  // even when the ref has not set `track` yet (see createPress).
  let focused = createMemo(() => {
    let id = focusedNode()
    return id != null && id === track?.id
  })
  let handleKeyDown = (e: KeyEvent) => {
    if (props.disabled) return
    let dir = e.key === "ArrowRight" || e.key === "ArrowUp" ? 1 : e.key === "ArrowLeft" || e.key === "ArrowDown" ? -1 : 0
    if (dir === 0) return
    e.stopPropagation()
    let inc = props.step ?? (max() - min()) / 100
    commit(clamp(value() + dir * inc, min(), max()))
  }

  return (
    <view
      transition={split().root}
      onTransitionEnd={transitionEndFor("root", props.onTransitionEnd)}
      ref={(n: { id: number }) => (track = n)}
      repaintBoundary
      flexDirection="row"
      alignItems="center"
      height={height()}
      width={theme.size.slider}
      {...props.layout}
      x={styled().x}
      y={styled().y}
      scale={styled().scale}
      rotate={styled().rotate}
      opacity={styled().opacity}
      pointerEvents={props.disabled ? "none" : "auto"}
      focusable={!props.disabled}
      onPointerDown={handleDown}
      onPointerMove={handleMove}
      onPointerUp={handleUp}
      onKeyDown={handleKeyDown}
    >
      {/* Colors fade with the theme; the fill's w and the thumb's x never
          get a transition - they track the drag 1:1, and a spring would
          rubber-band it. */}
      <view ref={(n: { id: number }) => (groove = n)} position="relative" flex={1} height={GROOVE}>
        <Surface
          role="control"
          fill={grooveColor()}
          radius={grooveRadius()}
          material={styled().material}
          glow={null}
          sunken
          outline={hasBorder() ? { color: styled().borderColor ?? theme.color.border, width: styled().borderWidth ?? theme.borderWidth.sm } : null}
          fillTransition={split().background}
          onFillTransitionEnd={transitionEndFor("background", props.onTransitionEnd)}
          outlineTransition={split().border}
          onOutlineTransitionEnd={transitionEndFor("border", props.onTransitionEnd)}
        />
        <d-rect
          transition={colorFade()}
          color={fillPaint().fill}
          w={fillPx()}
          h={GROOVE}
          radius={grooveRadius()}
          shadow={fillPaint().glow}
        />
        <view position="absolute" left={0} top={(GROOVE - thumb()) / 2} width={thumb()} height={thumb()} x={fillPx() - thumb() / 2}>
          <Surface
            role="control"
            elevation={styled().elevation ?? "raised"}
            fill={theme.color.thumb ?? theme.color.primary}
            radius={pill(thumb())}
            material={styled().material}
            glow={null}
            pressed={dragging()}
            tint={false}
            outline={focused() && policy.focusRing ? { color: theme.color.ring, width: theme.borderWidth.focus } : null}
          />
        </view>
      </view>
    </view>
  )
}
