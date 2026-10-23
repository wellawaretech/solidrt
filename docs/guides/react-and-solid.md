# React and SolidJS 2

SolidRT apps are written with [SolidJS 2](https://solidjs.com/). If
you know React, the JSX looks the same, but what runs when is different.
This page is a quick comparison; the SolidJS docs teach the model in full.

## The one big difference

A React component runs again on every state change and the result is diffed.
A Solid component runs once, to set things up. Reading a signal inside JSX
subscribes that one spot, and an update changes only that spot. There is no
re-render and no virtual DOM.

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

Most of the differences below follow from that.

## Side by side

| React | SolidJS 2 |
| --- | --- |
| `useState` | `createSignal`, read by calling it: `count()` |
| `useMemo` | `createMemo` |
| `useReducer`, nested state | `createStore`, updated through a draft: `setStore(s => { s.done = true })` |
| `useEffect` | `createEffect(compute, apply)`: `compute` tracks, `apply` does the side effect |
| `useEffect(..., [])` for mount and cleanup | `onSettled(() => { ...; return cleanup })` |
| `useRef` for a value that survives renders | A plain `let` in the component body: it runs once anyway |
| `useContext`, `<Ctx.Provider value>` | `useContext`, and the context is its own provider: `<Ctx value>` |
| Destructured props: `({ title }) =>` | `(props) =>`, and read `props.title` where you use it |
| `{cond && <X />}` | `<Show when={cond()}><X /></Show>` |
| `items.map(...)` with `key` | `<For each={items()}>`, keyed by identity, no `key` prop |
| `<Suspense>` | `<Loading>` |
| Error boundaries | `<Errored>` |
| Rules of hooks | None: create signals anywhere, in any order, even conditionally |

## Two traps

**Don't destructure props.** Props are reactive values. `props.title` reads
the current value each time it is accessed; `let { title } = props` reads it
once, and the component never sees an update.

**Pass values, not signals.** Write `<Counter value={count()} />`, not
`value={count}`. The JSX passes the call along reactively, and the child
reads a plain value from `props.value`.

## In SolidRT

Import everything from `@solidrt/core`, not from `solid-js`: it re-exports
the reactive and control-flow vocabulary next to the elements and the rest
of Core. The elements are native nodes, not DOM elements, so there is no
`className`, `style` or `onClick`; the
[element vocabulary](/core/vocabulary/) lists what there is instead.
