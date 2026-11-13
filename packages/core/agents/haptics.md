# Haptics (touch feedback by intent)

Read this before making the device respond to a touch, a toggle, an error
or a hit.

```ts
import { touchFeedback, stopTouchFeedback, touchFeedbackSupport, capabilities } from "@solidrt/core"
```

`touchFeedback(intent, options?)` plays the platform's own effect for
`intent` on the device's actuator. Name what happened, never how it should
feel: the platform picks the effect, so the same intent feels native on
every device. Call it at the moment the thing happens (in the handler that
flips the switch, rejects the input, lands the hit).

## Intents: pick by meaning

| intent | use it when |
|---|---|
| `selection` | the value moved to another discrete choice (a picker row, a slider step, a segmented control) |
| `alignment` | a dragged thing snapped to a guide or an edge |
| `thresholdOn` | a drag crossed the point where release acts (pull to refresh armed, swipe to delete armed) |
| `thresholdOff` | the drag crossed back before release |
| `toggleOn` | a switch turned on |
| `toggleOff` | a switch turned off |
| `longPress` | a long press triggered its action |
| `dragStart` | an item was picked up to drag |
| `success` | an action finished (saved, sent, unlocked) |
| `error` | an action failed, or an input was rejected |
| `warning` | an action needs care (a destructive confirm, a limit reached) |
| `impact` | a collision, a landing, a physical hit (games, physics) |

Rules:
- Pick the intent whose meaning matches the event, even when two feel the
  same on the device in your hand. On Android `alignment` plays the
  `selection` effect; on another platform they differ. Never choose an
  intent for its feel.
- A two-sided event is two names: `toggleOn` / `toggleOff`,
  `thresholdOn` / `thresholdOff`. Play the one for the direction it went.
- Feedback is for discrete events. Do not call it per frame or per pointer
  move: call `selection` once per step crossed, not once per move event.

## Impact options

Only `impact` takes options; every other intent accepts and ignores them.

```ts
touchFeedback("impact")                                        // medium, full strength
touchFeedback("impact", { weight: "heavy" })                   // "light" | "medium" | "heavy"
touchFeedback("impact", { weight: "light", intensity: 0.4 })   // intensity 0..1, default 1
```

`intensity` plays only where `touchFeedbackSupport().impactIntensity` is
true; elsewhere the weight's effect plays at its own strength. Scale it
from the event (a collision's speed), not from a guess at the device.

Validation throws: a TypeError on an unknown intent, a non-object
`options` or an unknown weight; a RangeError on an `intensity` that is not
a finite number in 0..1. Clamp a computed intensity before passing it.

`stopTouchFeedback()` stops an effect that still plays. Effects are short,
so most apps never call it; everything also stops when the app goes to the
background.

## Every call is safe everywhere

Where nothing plays, `touchFeedback` is a silent no-op: a device with no
actuator, the desktop, a TV. App code needs no platform branch and no
support check before a call. The user's system settings always win: with
touch feedback switched off, nothing plays, and no option overrides that.

## Support: read it once, only to design around it

- `capabilities.haptics` - the device has an actuator. A hardware fact,
  not reactive and not the user's settings.
- `touchFeedbackSupport()` - `{ actuator, intents, impactIntensity }`:
  `intents` maps each intent to `"native"` (the platform has its own
  effect), `"approximate"` (another intent's effect plays) or `"none"`
  (nothing plays). A frozen object, the same on every call.

Most apps never read either. Read support once, at startup, in an app
that changes its design when feedback cannot be felt (a game that adds
an audio or visual cue for hits where `impact` is `"none"`). An intent
the user has silenced is still `"native"`: support says what the device
can play, not what the user allows.

## The capability

The project's `package.json` lists `"haptics": true` under
`solidrt.capabilities` (the scaffold has it on). On Android it becomes
the `VIBRATE` permission. With `"haptics": false` the app has no
`VIBRATE`: the intents the system plays through the view (the touch
feedback constants) still play; the ones that need the vibrator
(`warning`, `impact`, and the fallbacks of older Android versions) report
`"none"`, play nothing, and the first such call logs one warning that
names the capability. Keep it on unless the app must not vibrate.

The old name `"vibration"` is refused by the project parser; rename the
key to `"haptics"`.

## Tests and verification

Under `sol test` nothing plays: each accepted call is recorded, and the
test reads them with `app.requestedHaptics()`, oldest first, the last 64,
each `{ intent, weight, intensity, at }` (`weight` and `intensity` are
null for every intent but `impact`; `at` is ms since the session began).
A test engine reports an actuator, so `capabilities.haptics` is true and
every intent is `"native"` there.

```ts
await app.input([{ type: "pointer", action: "tap", x: 100, y: 40 }])
await app.settle()
expect(app.requestedHaptics().map((h) => h.intent)).toEqual(["toggleOn"])
```

A dev client plays the effect and records it too: an agent reads the
calls back with the `get_requested_haptics` MCP tool (control API
`/__control__/requested-haptics`).
