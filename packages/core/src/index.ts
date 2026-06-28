export * from "./renderer"
export { setFocus, getFocusedNodeId, measureText, getBoundingBox } from "./core"
export type { BoundingBox } from "./core"
export { parseColor, createLinearGradient, createRadialGradient } from "./color"
export type { Gradient, GradientStop } from "./color"
export { onFrame, onLayout, onResize, onWindowFocus, onWindowBlur } from "./window"
export { windowSize, safeArea, displayScale, windowFocused, keyboardHeight } from "./window"
export { createTexture } from "./gpu"
export { createImage, decodeImage } from "./image"
export type { DecodedImage, ImageSource } from "./image"
export type {
  LayoutProps,
  TransformProps,
  PointerProps,
  PointerEvent,
  WheelEvent,
  KeyEvent,
  TextEvent,
  PaintProps,
  WindowProps,
  ViewProps,
  RectProps,
  OvalProps,
  LineProps,
  PathProps,
  TextProps,
  TextureProps,
  AudioProps,
  Color,
} from "./types"
export type { MeasureTextOptions } from "flux:rendertree"

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
