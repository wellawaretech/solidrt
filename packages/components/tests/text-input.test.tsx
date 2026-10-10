// Leaving a TextInput: Escape is a cancel (onCancel, then the blur) and the
// focused field consumes the key, so neither an enclosing onKeyDown nor the
// back stack, where an unconsumed Escape would land, sees it. A key the
// field does not keep bubbles on to the enclosing handler. Focus and key
// routing run in the client, so this is an app test.

import { test, expect } from "@solidrt/test"
import { onBack } from "@solidrt/core"
import { TextInput } from "../src/index.ts"

const FIELD_WIDTH = 200

test("Escape cancels the focused field and stops at it", async app => {
  let seen: string[] = []
  // Registered before the field focuses, so it sits beneath the field on the
  // back stack; the field never lets Escape get that far anyway.
  let offBack = onBack(() => seen.push("back"))
  try {
    await app.mount(() => (
      <view onKeyDown={e => seen.push(`parent ${e.key}`)}>
        <TextInput
          autoFocus
          onSubmit={() => seen.push("submit")}
          onCancel={() => seen.push("cancel")}
          onBlur={() => seen.push("blur")}
          layout={{ width: FIELD_WIDTH }}
        />
      </view>
    ))
    // autoFocus takes effect after mount.
    await app.settle()
    await app.key("Escape")
    expect(seen).toEqual(["cancel", "blur"])
  } finally {
    offBack()
  }
})

test("the user's back cancels the focused field and goes no further", async app => {
  let seen: string[] = []
  // Beneath the field on the stack: registered before it focuses. It
  // prevents, or the back would reach the platform default and exit.
  let offBack = onBack(e => {
    e.preventDefault()
    seen.push("app back")
  })
  try {
    await app.mount(() => (
      <TextInput
        autoFocus
        onCancel={() => seen.push("cancel")}
        onBlur={() => seen.push("blur")}
        layout={{ width: FIELD_WIDTH }}
      />
    ))
    await app.settle()
    await app.back()
    expect(seen).toEqual(["cancel", "blur"])

    // Blurred, the field's step is gone and the next back is the app's.
    await app.back()
    expect(seen).toEqual(["cancel", "blur", "app back"])
  } finally {
    offBack()
  }
})

test("a key the field does not keep bubbles to the enclosing handler", async app => {
  let seen: string[] = []
  await app.mount(() => (
    <view onKeyDown={e => seen.push(`parent ${e.key}`)}>
      <TextInput autoFocus layout={{ width: FIELD_WIDTH }} />
    </view>
  ))
  await app.settle()
  // Up in a single-line field is not the field's (a chat history's, say);
  // Left moves the caret and is.
  await app.key("ArrowUp")
  await app.key("ArrowLeft")
  expect(seen).toEqual(["parent ArrowUp"])
})
