// UI event bus (lattice), provided by the runtime as a builtin module, present
// in every lattice build. on/once return an unsubscribe function. Notable
// events: the routed pointer stream ("pointerMove"/"pointerDown"/... consumed
// by core's window.ts), "pointerFrame" (the move-batch terminator: fires after
// all of a frame's pointer moves have dispatched, every pointer the same age -
// multi-pointer recognizers measure there; its `timeStamp` is theirs),
// "render" (the per-frame signal), "availability" (sticky: the platform
// facts, `AvailabilityMap`), and "permission" (sticky: the user's standing
// answers, `PermissionMap`).
declare module "sol:events" {
  /**
   * Whether this platform has a facility, as `Flux.availability` reads it.
   * "available": a call is expected to work. "unavailable": a call cannot
   * work here and rejects with a "NotSupportedError"; hide the feature.
   * "unknown": the runtime cannot tell yet, either because a probe is still
   * in flight (it settles to one of the other two) or because only a call
   * can tell (the call settles it). Offer the feature unless it is
   * "unavailable", and handle the "NotSupportedError".
   */
  export type Availability = "available" | "unavailable" | "unknown"
  /** Every facility's availability, by its name. */
  export type AvailabilityMap = {
    link: Availability
    dialog: Availability
    share: Availability
    "clipboard-image": Availability
    haptics: Availability
    camera: Availability
    microphone: Availability
    audio: Availability
  }
  /**
   * The user's standing answer on a device permission, as
   * `navigator.permissions.query` reports it: "granted", "denied", or
   * "prompt" (the OS asks at the open, or at the first request).
   */
  export type PermissionState = "granted" | "denied" | "prompt"
  /** Every permission's state, by its name. */
  export type PermissionMap = {
    camera: PermissionState
    microphone: PermissionState
  }
  /**
   * The "availability" event is sticky: the engine emits the whole map at
   * start, so a subscriber gets it at once, and again on every change (a
   * probe settled, a call corrected the fact, a device came or went).
   */
  export function on(event: "availability", callback: (map: AvailabilityMap) => void): () => void
  /**
   * The "permission" event is sticky: the engine emits the whole map at
   * start, so a subscriber gets it at once, and again on every change (the
   * user answered a prompt, an open was refused, the app came back to the
   * foreground with a new answer).
   */
  export function on(event: "permission", callback: (map: PermissionMap) => void): () => void
  export function on(event: string, callback: (data: any) => void): () => void
  export function once(event: "availability", callback: (map: AvailabilityMap) => void): () => void
  export function once(event: "permission", callback: (map: PermissionMap) => void): () => void
  export function once(event: string, callback: (data: any) => void): () => void
}
