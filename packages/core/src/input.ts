// The runtime-free half of the input surface, importable as
// `@solidrt/core/input` by modules that must stay headless (the camera
// controls and their checks run on the bare flux binary, which has no
// event bus). The map, the control-side axes contract and the keyboard
// device are pure; the gamepad device and the pointer feed need the
// runtime and live on the main entry.
export { createInputMap } from "./input-map"
export type { ActionKind, ActionValue, ActionsDecl, Binding, DeltaSink, DeviceName, GestureListener, InputDevice, InputDeviceSet, InputMap, InputSource, RebindOptions, RebindPart } from "./input-map"
export { chord, doubleTap, hold, invert, scale, tap } from "./input-processors"
export { resolveSource } from "./input-id"
export { createAxes, combineRates } from "./input-axes"
export type { Axes, AxesDecl, AxesHooks, AxisKind, AxisValue, Vec2 } from "./input-axes"
export { keyboard } from "./input-keyboard"
export type { KeyboardDevice, KeyboardVec2Keys } from "./input-keyboard"
