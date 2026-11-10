// The `detached` attribute (okf/done/detached-attribute.md), through the real
// bundler: the pass in packages/cli/src/bundle/detached.ts turns a literal
// `detached` at a host tag into the detached form before the solid plugin
// runs, so the node is built detached even where a spread stops the
// compiler inlining static props. On a component the name is an ordinary
// prop; inside a spread it throws.

import { test, expect } from "@solidrt/test"
import { Errored } from "@solidrt/core"

const SIDE = 40
const COLOR = "#ffffff"
const SHAPE = "M0 0 L10 0 L10 10 Z"
const MESSAGE = "Detached is set once, as a literal attribute at the tag"

test("a literal detached builds the detached form; a plain tag stays laid out", async app => {
  let drawn = app.ref()
  let laid = app.ref()
  await app.mount(() => (
    <view width={SIDE} height={SIDE}>
      <rect ref={drawn} detached color={COLOR} />
      <rect ref={laid} color={COLOR} />
    </view>
  ))
  expect(drawn.record.kind).toBe("rect")
  expect(drawn.record.detached).toBe(true)
  expect(laid.record.kind).toBe("rect")
  expect(laid.record.detached).toBe(undefined)
})

test("detached next to a spread that does not carry it still builds the detached form", async app => {
  let draw = { d: SHAPE, color: COLOR }
  let shape = app.ref()
  await app.mount(() => (
    <view width={SIDE} height={SIDE}>
      <path ref={shape} detached {...draw} />
    </view>
  ))
  expect(shape.record.kind).toBe("path")
  expect(shape.record.detached).toBe(true)
})

test("a detached view with a spread takes a static detached child", async app => {
  let p = { x: SIDE }
  let outer = app.ref()
  let inner = app.ref()
  await app.mount(() => (
    <view width={SIDE} height={SIDE}>
      <view ref={outer} {...p} detached>
        <rect ref={inner} detached color={COLOR} />
      </view>
    </view>
  ))
  expect(outer.record.detached).toBe(true)
  expect(inner.record.detached).toBe(true)
})

test("detached={false} keeps the laid-out form", async app => {
  let box = app.ref()
  await app.mount(() => (
    <view width={SIDE} height={SIDE}>
      <rect ref={box} detached={false} color={COLOR} />
    </view>
  ))
  expect(box.record.kind).toBe("rect")
  expect(box.record.detached).toBe(undefined)
})

function Card(props: { detached?: boolean }) {
  return <text>{props.detached === true ? "Detached prop" : "No prop"}</text>
}

test("a component gets detached as an ordinary prop", async app => {
  let win = await app.mount(() => <Card detached />)
  expect(win.find({ text: "Detached prop" }).exists).toBe(true)
})

test("detached inside a spread throws", async app => {
  let p = { detached: true } as any
  let win = await app.mount(() => (
    <Errored fallback={err => <text>{(err() as Error).message}</text>}>
      <view {...p} />
    </Errored>
  ))
  await app.settle()
  expect(win.find({ text: MESSAGE }).exists).toBe(true)
})
