export * from "./renderer"
export { setFocus, focusedNode, startTextInput, textInputActive, getFocusables, measureText, getBoundingBox, getBoundingBoxViewport } from "./core"
export type { BoundingBox } from "./core"
export { parseColor, mixColors, brightness, createLinearGradient, createRadialGradient } from "./color"
export type { Gradient, GradientStop } from "./color"
export { onFrame, onLayout, onResize, onWindowFocus, onWindowBlur, onBack, exit } from "./window"
export type { BackEvent } from "./window"
export { windowSize, safeArea, displayScale, windowFocused, keyboardHeight } from "./window"
export { env } from "./environment"
export type { InputDevices, SystemTheme, Orientation, Visibility } from "./environment"
export { gamepads } from "./gamepad"
export type { GamepadState } from "./gamepad"
export { capabilities } from "./capabilities"
export type { Capabilities, WindowSizeClass } from "./capabilities"
export { createTexture } from "./gpu"
export type { TextureId } from "./gpu"
export { createImage, decodeImage } from "./image"
export type { DecodedImage, ImageSource } from "./image"
export { parseSvg, svg } from "./svg"
export type { SvgDocument, SvgDraw } from "./svg"
export { createScroll } from "./scroll"
export type { Scroll, ScrollAxis, ScrollOffset, ScrollOptions } from "./scroll"
export type {
  LayoutProps,
  TransformProps,
  PointerProps,
  PointerEvent,
  WheelEvent,
  KeyEvent,
  TextEvent,
  TextInputHints,
  PaintProps,
  WindowProps,
  WindowShaderProps,
  ViewProps,
  RectProps,
  OvalProps,
  LineProps,
  PathProps,
  TextProps,
  TextureProps,
  Color,
  Pct,
} from "./types"
export type { MeasureTextOptions } from "flux:rendertree"

// A percentage value for dimensional props (e.g. transformOrigin): `pct(50)` is
// half the element box. Keeps percentages a first-class branded value rather
// than a string that has to be parsed - a bare number stays pixels.
export let pct = (v: number): import("./types").Pct => ({ __unit: "pct", v })

// --- Authoring-surface re-exports -------------------------------------------
// A SolidRT app is built from three substrate packages: @solidjs/signals
// (reactivity), solid-js (control-flow components), and @solidjs/universal (the
// renderer factory, surfaced via ./renderer). Forwarding the app-facing pieces
// here means an app imports its whole vocabulary from "@solidrt/core" instead of
// having to know which substrate package each symbol lives in. These are already
// peerDependencies, so this adds no new dependency. Curated on purpose - do not
// `export *` from solid-js, which would leak DOM/hydration-only helpers that are
// meaningless on the flux runtime.

// Reactivity (from @solidjs/signals).
export {
  createSignal,
  createMemo,
  createEffect,
  createRenderEffect,
  createRoot,
  createStore,
  reconcile,
  mapArray,
  repeat,
  untrack,
  flush,
  onCleanup,
  onSettled,
} from "@solidjs/signals"
export type { Accessor, Setter, Signal, Store, StoreSetter } from "@solidjs/signals"

// Control flow, components, and context (from solid-js).
export {
  For,
  Show,
  Switch,
  Match,
  Repeat,
  Loading,
  Errored,
  Reveal,
  lazy,
  createUniqueId,
  createContext,
  useContext,
  children,
} from "solid-js"
export type {
  Component,
  ParentComponent,
  FlowComponent,
  VoidComponent,
  ComponentProps,
} from "solid-js"

// --- Removed in Solid 2.0 (deprecation stubs) -------------------------------
// These symbols no longer exist in Solid 2.0. They are re-exported as `never`
// so that imports keep resolving and the IDE surfaces a strikethrough plus the
// migration hint instead of an opaque "not exported" error. See CHEATSHEET.md
// "Removed (with replacements)".

/**
 * @deprecated Removed in Solid 2.0. Use `<For keyed={false}>` - `item` becomes
 * an accessor and `i` a plain number. See CHEATSHEET.md "Removed".
 */
export const Index: never = undefined as never

/**
 * @deprecated Removed in Solid 2.0. Default microtask batching applies; call
 * `flush()` to apply pending writes synchronously.
 */
export const batch: never = undefined as never

/**
 * @deprecated Removed in Solid 2.0. Use `createMemo`, a split `createEffect`, or
 * the function-form `createSignal`.
 */
export const createComputed: never = undefined as never

/**
 * @deprecated Removed in Solid 2.0. Use async computations with `<Loading>`,
 * e.g. `createMemo(() => fetchX(id()))`.
 */
export const createResource: never = undefined as never

/**
 * @deprecated Removed in Solid 2.0. Use built-in transitions: `isPending`,
 * `<Loading>`, or the optimistic APIs.
 */
export const startTransition: never = undefined as never

/**
 * @deprecated Removed in Solid 2.0. Use built-in transitions: `isPending`,
 * `<Loading>`, or the optimistic APIs.
 */
export const useTransition: never = undefined as never

/**
 * @deprecated Removed in Solid 2.0. Use split effects - the compute phase makes
 * dependencies explicit.
 */
export const on: never = undefined as never

/**
 * @deprecated Removed in Solid 2.0. Use `<Errored>` or the effect `error` option.
 */
export const onError: never = undefined as never

/**
 * @deprecated Removed in Solid 2.0. Use `<Errored>` or the effect `error` option.
 */
export const catchError: never = undefined as never

/**
 * @deprecated Removed in Solid 2.0. Store setters are draft-first by default.
 */
export const produce: never = undefined as never

/**
 * @deprecated Removed in Solid 2.0. Use `createStore` with draft setters.
 */
export const createMutable: never = undefined as never

/**
 * @deprecated Removed in Solid 2.0. Use `createStore` with draft setters.
 */
export const modifyMutable: never = undefined as never

/**
 * @deprecated Removed in Solid 2.0. Use async iterables in computations, or a
 * `createEffect` to push values out.
 */
export const from: never = undefined as never

/**
 * @deprecated Removed in Solid 2.0. Use async iterables in computations, or a
 * `createEffect` to push values out.
 */
export const observable: never = undefined as never

/**
 * @deprecated Removed in Solid 2.0. Use `mapArray`, which handles non-keyed too.
 */
export const indexArray: never = undefined as never

/**
 * @deprecated Removed in Solid 2.0. Error boundaries heal automatically.
 */
export const resetErrorBoundaries: never = undefined as never
