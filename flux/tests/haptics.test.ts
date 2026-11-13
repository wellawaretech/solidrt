// flux:haptics. The test host records feedback instead of playing it, so
// every call returns at once with nothing felt, and support reports a
// device that plays every intent natively, whatever the machine has.
import { expect, test } from "flux:test"
import { feedback, stopFeedback, feedbackSupport } from "flux:haptics"
import type { HapticIntent, ImpactWeight } from "flux:haptics"

const INTENTS: HapticIntent[] = [
  "selection",
  "alignment",
  "thresholdOn",
  "thresholdOff",
  "toggleOn",
  "toggleOff",
  "longPress",
  "dragStart",
  "success",
  "error",
  "warning",
  "impact",
]
const WEIGHTS: ImpactWeight[] = ["light", "medium", "heavy"]
// An intensity inside 0..1 that is neither end.
const MID_INTENSITY = 0.5

test("the test host lists the haptics capability", () => {
  expect(Flux.capabilities.includes("haptics")).toBe(true)
})

test("feedbackSupport reports an actuator playing every intent natively, frozen", () => {
  let facts = feedbackSupport()
  expect(facts.actuator).toBe(true)
  expect(facts.impactIntensity).toBe(true)
  expect(Object.keys(facts.intents)).toEqual(INTENTS)
  for (let intent of INTENTS) expect(facts.intents[intent]).toBe("native")
  expect(Object.isFrozen(facts)).toBe(true)
  expect(Object.isFrozen(facts.intents)).toBe(true)
})

test("feedback returns undefined for every intent", () => {
  for (let intent of INTENTS) expect(feedback(intent)).toBeUndefined()
})

test("feedback takes an impact's weight and intensity", () => {
  for (let weight of WEIGHTS) expect(feedback("impact", { weight })).toBeUndefined()
  expect(feedback("impact", { intensity: MID_INTENSITY })).toBeUndefined()
  expect(feedback("impact", { weight: "heavy", intensity: 0 })).toBeUndefined()
  expect(feedback("impact", { weight: "light", intensity: 1 })).toBeUndefined()
  expect(feedback("impact", {})).toBeUndefined()
})

test("feedback throws a TypeError on an unknown or non-string intent", () => {
  let call = feedback as (intent: unknown, options?: unknown) => void
  expect(() => call("buzz")).toThrow(TypeError)
  expect(() => call("buzz")).toThrow("feedback: unknown intent \"buzz\"; one of selection, alignment, thresholdOn")
  expect(() => call(7)).toThrow(TypeError)
  expect(() => call(undefined)).toThrow(TypeError)
})

test("feedback throws a TypeError on non-object options and an unknown weight", () => {
  let call = feedback as (intent: unknown, options?: unknown) => void
  expect(() => call("impact", "heavy")).toThrow(TypeError)
  expect(() => call("impact", 1)).toThrow("feedback: options must be an object")
  expect(() => call("impact", { weight: "huge" })).toThrow(TypeError)
  expect(() => call("impact", { weight: "huge" })).toThrow("feedback: unknown weight \"huge\"")
  expect(() => call("impact", { weight: 2 })).toThrow(TypeError)
})

test("feedback throws a RangeError on an intensity outside 0..1", () => {
  let call = feedback as (intent: unknown, options?: unknown) => void
  expect(() => call("impact", { intensity: 1.5 })).toThrow(RangeError)
  expect(() => call("impact", { intensity: -0.1 })).toThrow(RangeError)
  expect(() => call("impact", { intensity: NaN })).toThrow(RangeError)
  expect(() => call("impact", { intensity: Infinity })).toThrow(RangeError)
  expect(() => call("impact", { intensity: "1" })).toThrow("feedback: intensity must be a finite number in 0..1")
})

test("stopFeedback returns undefined", () => {
  expect(stopFeedback()).toBeUndefined()
})
