// A mounted Modal is one step of the back stack. With nothing focused an
// Escape reaches the end of the key chain unconsumed and runs that stack:
// the modal closes and a handler the app registered beneath it never hears
// the press. Not dismissable, the modal still takes the step. Unmounted, its
// step is gone and the press reaches the app's handler.

import { test, expect } from "@solidrt/test"
import { createSignal, onBack, Show } from "@solidrt/core"
import { Modal, Text, TextInput } from "../src/index.ts"

const FIELD_WIDTH = 200

test("Escape closes the modal through the back stack, beneath-handlers unheard", async app => {
  let [open, setOpen] = createSignal(false)
  let seen: string[] = []
  // Registered before the modal mounts: beneath it on the stack.
  let offBack = onBack(() => seen.push("app back"))
  try {
    await app.mount(() => (
      <Show when={open()}>
        <Modal
          onClose={() => {
            seen.push("close")
            setOpen(false)
          }}
        >
          <Text>Remove it?</Text>
        </Modal>
      </Show>
    ))
    // A portal cannot mount during the initial render.
    setOpen(true)
    await app.settle()
    expect(app.find({ text: "Remove it?" }).visible).toBe(true)

    await app.key("Escape")
    await app.settle()
    expect(seen).toEqual(["close"])
    expect(app.find({ text: "Remove it?" }).visible).toBe(false)

    // The modal's step left with it.
    await app.key("Escape")
    expect(seen).toEqual(["close", "app back"])
  } finally {
    offBack()
  }
})

test("a back leaves a focused field before it closes the modal", async app => {
  let [open, setOpen] = createSignal(false)
  let seen: string[] = []
  // The app's own step, beneath both; it prevents, or the back would reach
  // the platform default and exit.
  let offBack = onBack(e => {
    e.preventDefault()
    seen.push("app back")
  })
  try {
    await app.mount(() => (
      <Show when={open()}>
        <Modal
          onClose={() => {
            seen.push("close")
            setOpen(false)
          }}
        >
          <TextInput autoFocus onCancel={() => seen.push("cancel")} layout={{ width: FIELD_WIDTH }} />
        </Modal>
      </Show>
    ))
    setOpen(true)
    await app.settle()

    // The field registered at focus, after the modal at mount: it is the top.
    await app.back()
    expect(seen).toEqual(["cancel"])
    await app.back()
    expect(seen).toEqual(["cancel", "close"])
    await app.settle()
    await app.back()
    expect(seen).toEqual(["cancel", "close", "app back"])
  } finally {
    offBack()
  }
})

test("a modal that is not dismissable takes the back and stays", async app => {
  let [open, setOpen] = createSignal(false)
  let seen: string[] = []
  let offBack = onBack(() => seen.push("app back"))
  try {
    await app.mount(() => (
      <Show when={open()}>
        <Modal dismissable={false} onClose={() => seen.push("close")}>
          <Text>Answer me</Text>
        </Modal>
      </Show>
    ))
    setOpen(true)
    await app.settle()

    await app.key("Escape")
    await app.settle()
    expect(seen).toEqual([])
    expect(app.find({ text: "Answer me" }).visible).toBe(true)
  } finally {
    offBack()
  }
})
