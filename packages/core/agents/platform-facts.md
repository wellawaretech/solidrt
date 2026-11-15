# Platform facts (what this platform has, what the user allows)

Read this before gating a feature on the platform or on the user's
answer: a file dialog, a link, the share sheet, the clipboard image,
haptics, a camera, a microphone.

```ts
import { availability, permission } from "@solidrt/core"
```

## Four questions, four answers

Whether a feature works is four separate questions. Each has its own
answer; never fold them into one check, and never guess from the OS.

| question | answer | changes |
|---|---|---|
| Is the module in this build? | `Flux.capabilities.includes("camera")` | never: frozen for the process |
| Does this platform have it? | `availability("camera")` | reactive: a probe settles, a device comes or goes, a call corrects it |
| Did the user allow it? | `permission("camera")` (`Flux.permission`) | reactive: the user answers a prompt, or changes it in the system settings |
| Does the project use it? | `solidrt.uses` in package.json | at pack time; a name switched off leaves `Flux.capabilities` |

The platform names are `link`, `dialog`, `share`, `clipboard-image`,
`haptics`, `camera`, `microphone` and `audio`. Any other name throws a
TypeError. camera, microphone and audio are build names too
(`Flux.capabilities`); the other five are platform facts only, and the
modules behind them (`flux:link`, `flux:dialog`, `navigator.share`,
`navigator.clipboard`, `flux:haptics`) exist on every build.

## Availability: three states

- `"available"`: the platform says yes. A call is expected to work.
- `"unavailable"`: a call cannot work here. It rejects with a
  `NotSupportedError`. Hide the feature.
- `"unknown"`: the runtime cannot tell yet. Either a probe is in flight
  (it settles to one of the other two), or only a call can tell (the
  call settles it).

## The rule: show unless unavailable

Offer the feature unless it is `"unavailable"`, and handle the
`NotSupportedError` the call may still reject with. `"unknown"` never
hides a feature that would have worked. Gating on `=== "available"`
hides a working button on every platform that settles late.

```tsx
async function share() {
  try {
    await navigator.share({ text: "My board" })
  } catch (e) {
    // The user closed the sheet, or this platform has none after all.
    let name = (e as Error).name
    if (name !== "AbortError" && name !== "NotSupportedError") throw e
  }
}

<Show when={availability("share") !== "unavailable"}>
  <view onPointerDown={share}>
    <text>Share</text>
  </view>
</Show>
```

A call corrects the fact: when a call fails because the facility is not
there, the state reads `"unavailable"` before the call rejects, and a
call that worked on an `"unknown"` facility reads `"available"`. So the
button above hides itself after the first failed call, and the second
tap never repeats the surprise.

Read `availability(name)` in a tracked scope (JSX, a memo, an effect's
compute) so the UI follows a probe settling or a device arriving. A read
at the top of a component body freezes at its first value.
`capabilities.haptics` is `availability("haptics") === "available"`:
haptics is probed once at start, so it is never `"unknown"`.

## Permission: the user's standing answer

`permission(name)` is whether the user allows the app a device. The
names are `camera` and `microphone`; any other name throws a TypeError.
Haptics and internet are not permissions: no platform asks the user for
them. A name the project switched off in `solidrt.uses` is not a
permission either: `permission(name)` throws a TypeError on it.

- `"granted"`: the user allowed it. An open is expected to stream.
- `"denied"`: the user refused. An open fails until the user allows it
  in the system settings. Tell the user where.
- `"prompt"`: the user has not answered. The OS asks at the next open.

There is no request call. Ask by opening: the open is the request. A
`"prompt"` camera is a camera to offer; the open shows the OS prompt,
and the answer moves the state to `"granted"` or `"denied"`.

```tsx
// Scanner opens the camera when it mounts: on "prompt" that open asks.
<Show when={permission("camera") !== "denied"} fallback={<text>Allow the camera in the system settings</text>}>
  <Scanner />
</Show>
```

The permission is not the open's progress. `createCamera` and
`createMicrophone` report how one open went through their `error()`,
as before. `permission(name)` is the standing answer before and after
any open, and a denied open settles it to `"denied"`.

Read `permission(name)` in a tracked scope, as `availability(name)`.
Before the engine's first report commits, the engine's own read
`Flux.permission(name)` answers, so the first value is already exact.
Outside a component, `Flux.permission(name)` is the same read without
the reactivity, and the sticky `permission` event of `sol:events`
carries the whole map on every change.

What each platform reports:

- Android: exact. `"prompt"` before the first ask, `"granted"` or
  `"denied"` after the answer, `"denied"` after "don't ask again", and
  `"prompt"` again when an "only this time" grant ends. The state is read
  again when the app comes back to the foreground.
- macOS and iOS: the system's authorization status.
- Windows: the privacy switches. `"granted"` or `"denied"`, never
  `"prompt"`.
- Linux outside a sandbox: `"granted"`, there is nothing to ask. In
  Flatpak the camera is `"prompt"`: the portal decides at the open.

## Tests and verification

Under `sol test` link, dialog, share, clipboard-image and haptics read
`"available"` (the test host records those calls and shows nothing);
camera, microphone and audio read `"unknown"`. A test sets another state
with `app.setAvailability(name, state)`, and a reader updates after the
next frame or `settle()`:

```tsx
await app.mount(() => <text label="dialog">{availability("dialog")}</text>)
app.setAvailability("dialog", "unavailable")
await app.settle()
expect(app.find({ label: "dialog" }).text).toBe("unavailable")
```

A test sets the permission with `app.setPermission(name, state)`
(`"granted"`, `"denied"` or `"prompt"`), and a reader updates in the same
way:

```tsx
await app.mount(() => <text label="camera">{permission("camera")}</text>)
app.setPermission("camera", "denied")
await app.settle()
expect(app.find({ label: "camera" }).text).toBe("denied")
```

On a running client, the MCP `get_availability` and `get_permission`
tools (the control API's `/availability` and `/permission`) read the
live maps; `list_clients` has each client's platform facts and
permissions as they were at connect.
