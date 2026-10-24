# Coming from React

You write SolidRT apps with [SolidJS 2](https://solidjs.com/). If you know
React, the JSX looks the same. But the time when code runs is different.
This page gives a short comparison. The SolidJS docs give a full
description of the model.

## The primary difference

A React component runs again at each state change, and React compares the
result with the previous result. A Solid component runs one time, to set up
the app. When JSX reads a signal, that one location in the JSX connects to
the signal. An update changes only that location. Solid does not render
again and does not use a virtual DOM.

```tsx
function Counter() {
  let [count, setCount] = createSignal(0)
  console.log("runs once")
  return (
    <view onPointerDown={() => setCount(count() + 1)}>
      <text>Tapped {count()} times</text>
    </view>
  )
}
```

Most of the differences in the table below come from this difference.

## Comparison table

| React | SolidJS 2 |
| --- | --- |
| `useState` | `createSignal`. To read it, call it: `count()` |
| `useMemo` | `createMemo` |
| `useReducer`, nested state | `createStore`. Update it through a draft: `setStore(s => { s.done = true })` |
| `useEffect` | `createEffect(compute, apply)`. `compute` tracks, and `apply` does the side effect |
| `useEffect(..., [])` for mount and cleanup | `onSettled(() => { ...; return cleanup })` |
| `useRef` for a value that stays between renders | A usual `let` in the component body. The body runs only one time |
| `useRef` for an element | `ref={el => (box = el)}` |
| `useCallback` | A usual function. The body runs only one time |
| `useContext`, `<Ctx.Provider value>` | `useContext`. The context is also its provider: `<Ctx value>` |
| Destructured props: `({ title }) =>` | `(props) =>`. Read `props.title` where you use it |
| `{cond && <X />}` | `<Show when={cond()}><X /></Show>` |
| `items.map(...)` with `key` | `<For each={items()}>`. It uses identity as the key. For a different key, use `keyed={item => item.id}` |
| `<Suspense>` | `<Loading>` |
| Error boundaries | `<Errored>` |
| Rules of hooks | No rules about where to make signals. Make them in any sequence, also in conditions |

## Three usual errors

**Do not destructure props.** Props are reactive values. Each time you read
`props.title`, you get the current value. `let { title } = props` reads the
value only one time. Then the component does not see updates.

**Give values, not signals.** Write `<Counter value={count()} />`, not
`value={count}`. The JSX gives the call to the child reactively. The child
reads a usual value from `props.value`.

**Do not read a signal at the top of the component body.** The body runs
one time, so the read gets only the first value. Read the signal in the
JSX, or in `createMemo`.

## In SolidRT

Import all names from `@solidrt/core`, not from `solid-js`. `@solidrt/core`
exports the reactive and control-flow names again, together with the
elements and the remaining parts of Core. The elements are native nodes, not
DOM elements. Thus, there is no `className`, `style` or `onClick`. The
[element vocabulary](/core/vocabulary/) shows what to use.
