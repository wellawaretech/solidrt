// navigator.permissions: the web's Permissions API, in every engine, for the
// user's standing answer on a device permission. Solidrt semantics on the
// web's shape:
// - Two names: "camera" and "microphone". Any other name rejects with a
//   TypeError, as the web rejects a name it does not support. So does a
//   name the app's project declared it does not use (`uses.camera: false`):
//   that is a packaging fact, not a permission state.
// - The state is "granted", "denied" or "prompt". "prompt" is the honest
//   state wherever the OS asks at the open (macOS before the first open,
//   Android before the first request, the camera inside a Flatpak sandbox).
//   Where a platform has no prompt for a name (Linux outside a sandbox),
//   the state is "granted". Windows reads its privacy switches: "granted"
//   or "denied", never "prompt".
// - The status is live: `state` reads the current answer on every access,
//   and "change" fires when the state differs from the last one the status
//   saw (the user answered a prompt, an open was refused, the app came back
//   from the system settings with a new answer).
// - There is no request(): opening the camera or the microphone is the
//   request, and the open reports what the OS decided. A refused open
//   settles the state to "denied".
// - No Event or EventTarget: a listener gets a plain
//   `{ type: "change", target, currentTarget }` object, and only function
//   listeners are taken (no `handleEvent` objects, no options).
// - A status with a listener (`onchange` or "change") holds the engine
//   like any event listener; remove it when done.
// Under `sol test` a test sets the state with `setPermission` (sol:test).

type PermissionName = "camera" | "microphone"
type PermissionState = "granted" | "denied" | "prompt"

interface PermissionDescriptor {
  name: PermissionName
}

interface PermissionStatusChangeEvent {
  readonly type: "change"
  readonly target: PermissionStatus
  readonly currentTarget: PermissionStatus
}

interface PermissionStatus {
  /** The permission this status reports. */
  readonly name: PermissionName
  /** The user's standing answer now, read on every access. */
  readonly state: PermissionState
  /** Called when the state changes; null for none. */
  onchange: ((this: PermissionStatus, event: PermissionStatusChangeEvent) => void) | null
  /** Calls `listener` when the state changes. A listener added twice is called once. */
  addEventListener(type: "change", listener: (this: PermissionStatus, event: PermissionStatusChangeEvent) => void): void
  removeEventListener(type: "change", listener: (this: PermissionStatus, event: PermissionStatusChangeEvent) => void): void
}

interface Permissions {
  /**
   * Resolves to a live status of the named permission. Rejects with a
   * TypeError when `descriptor` is not an object with a string `name`, when
   * the name is not "camera" or "microphone", and when the app's project
   * declared it does not use it.
   */
  query(descriptor: PermissionDescriptor): Promise<PermissionStatus>
}

interface Navigator {
  readonly permissions: Permissions
}
