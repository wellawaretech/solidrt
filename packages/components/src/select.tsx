import { createEffect, createSignal, createPortal, onBack, onCleanup, onLayout, getBoundingBox, Show, For, env, untrack } from "@solidrt/core"
import type { BoundingBox, LayoutProps, Scroll } from "@solidrt/core"
import { createPress } from "./press"
import { pushNavScope } from "./focus-nav"
import { placePopup, PopupList } from "./popup"
import { theme } from "./theme"
import { policy } from "./policy"
import { space } from "./spacing"
import { typeStyle } from "./typography"
import type { Option, StyleProps, TransitionProps } from "./types"
import { splitTransition, transitionEndFor } from "./types"
import { Icon } from "./icon"
import { colorFade, popupFadeOut, PressFeedback, travelMotion } from "./motion"
import { Surface } from "./surface"

export interface SelectProps<T = unknown> extends TransitionProps {
  options: Option<T>[]
  // Controlled selected value. If omitted, the select is uncontrolled.
  value?: T
  defaultValue?: T
  onChange?: (value: T) => void
  // Shown in the trigger while nothing is selected.
  placeholder?: string
  disabled?: boolean
  layout?: LayoutProps
  style?: StyleProps
}

// Scrim kept visible above a bottom sheet at its tallest.
let sheetClearance = () => theme.spacing.xl

/**
 * A single-choice picker whose presentation forks on the interaction policy:
 * desktop/hybrid opens an anchored dropdown under the trigger (above it when
 * only that side has the room), touch opens a bottom sheet over a scrim.
 * Same value/onChange contract either way; pressing outside, Escape and the
 * user's back close without a change. A list taller than the room scrolls,
 * opening with the selected option in view, and while open the options are
 * the focus navigation's scope.
 */
export function Select<T>(props: SelectProps<T>) {
  let trigger: { id: number } | undefined
  // The selected option's row while a presentation is open.
  let selectedRow: { id: number } | undefined
  let [open, setOpen] = createSignal(false)
  // The value overload: as far as TypeScript knows T could be a function,
  // which createSignal would read as a compute; the cast picks the value form.
  let [internal, setInternal] = createSignal<T | undefined>(props.defaultValue as Exclude<T | undefined, Function>)
  let value = () => (props.value !== undefined ? props.value : internal())
  let selected = () => props.options.find((o) => o.value === value())

  let choose = (v: T) => {
    setOpen(false)
    if (props.value === undefined) setInternal(() => v)
    props.onChange?.(v)
  }

  // Open, the picker is one step of core's back stack: Escape and the user's
  // back close it as a press outside does, without a change.
  createEffect(
    () => open(),
    (isOpen) => {
      if (!isOpen) return
      return onBack((e) => {
        e.preventDefault()
        setOpen(false)
      })
    },
  )

  let bodyText = (color: string) => ({ ...typeStyle("body"), color, maxLines: 1 })

  // One option row, shared by both presentations; only the vertical padding
  // differs (the sheet gets taller touch targets).
  let OptionRow = (p: { option: Option<T>; padY: number }) => {
    let press = createPress({ onPress: () => choose(p.option.value) })
    return (
      <view
        ref={(n: { id: number }) => {
          press.ref(n)
          // A mount-time snapshot: the row that is selected as the list opens.
          if (p.option.value === untrack(value)) selectedRow = n
        }}
        repaintBoundary
        flexDirection="row"
        alignItems="center"
        paddingTop={p.padY}
        paddingBottom={p.padY}
        paddingLeft={space("md")}
        paddingRight={space("md")}
        {...press.handlers}
        cursor="pointer"
        focusable
      >
        <PressFeedback
          pressed={press.pressed()}
          hovered={(press.hovered() && policy.interaction !== "touch") || (press.focused() && policy.focusRing)}
        />
        <text transition={colorFade()} {...bodyText(p.option.value === value() ? theme.color.primary : theme.color.text)}>{p.option.label}</text>
      </view>
    )
  }

  // Once per opening, once the list has its final size (the scroll range
  // matches the rows' overflow, so the write is not clamped against a stale
  // range): the selected row to the middle of the list, without motion.
  let createReveal = () => {
    let done = false
    return (scroll: Scroll | undefined, list: BoundingBox, height: number) => {
      if (done || !scroll) return
      let overflow = Math.max(0, list.height - height)
      if (overflow === 0) {
        done = true
        return
      }
      if (scroll.range().y !== overflow) return
      let row = selectedRow && getBoundingBox(selectedRow)
      done = true
      if (row) scroll.scrollTo({ y: row.y - list.y - (height - row.height) / 2, behavior: "instant" })
    }
  }

  // Anchored under the trigger, sized at least as wide as it, on the side
  // with room (placePopup): portal at the window root, pinned at 0,0 and
  // moved with the x/y paint transforms after measuring, so tracking the
  // anchor never reflows. The rows scroll once taller than the room. Glows
  // with theme.glow.overlay (the sheet sits on a scrim and does not).
  let Dropdown = () => {
    let menu: { id: number } | undefined
    let list: { id: number } | undefined
    let scroll: Scroll | undefined
    let [pos, setPos] = createSignal<{ x: number; y: number } | null>(null)
    let [minWidth, setMinWidth] = createSignal(0)
    let [listHeight, setListHeight] = createSignal<number | null>(null)
    let reveal = createReveal()
    selectedRow = undefined
    // The padding above and below the rows: the popup's height beyond the list's.
    let chrome = () => 2 * theme.spacing.sm
    onLayout(() => {
      let a = trigger && getBoundingBox(trigger)
      let b = menu && getBoundingBox(menu)
      let l = list && getBoundingBox(list)
      if (!a || !b || !l) return
      if (a.width !== minWidth()) setMinWidth(a.width)
      let p = placePopup(a, { width: b.width, height: l.height + chrome() })
      let h = Math.min(l.height, p.room - chrome())
      if (h !== listHeight()) setListHeight(h)
      let cur = pos()
      if (!cur || cur.x !== p.x || cur.y !== p.y) setPos({ x: p.x, y: p.y })
      reveal(scroll, l, h)
    })
    // Open, the options are the focus navigation's scope (as a Modal is), so
    // a pad or keyboard cannot walk onto the screen behind the dropdown.
    let popScope: (() => void) | undefined
    onCleanup(() => popScope?.())
    // The fade is driven by the position write, not a mount-time `from`: the
    // menu parks offscreen until measured, so a `from` would play unseen.
    // The exit fade lives on this root (the node the close unmounts).
    return createPortal(
      <view
        ref={(n: { id: number }) => (popScope = pushNavScope(n))}
        position="absolute"
        top={0}
        left={0}
        right={0}
        bottom={0}
        opacity={pos() ? 1 : 0}
        transition={popupFadeOut()}
      >
        <view position="absolute" top={0} left={0} right={0} bottom={0} onPointerDown={() => setOpen(false)} />
        <view
          ref={(n: { id: number }) => (menu = n)}
          position="absolute"
          top={0}
          left={0}
          x={pos()?.x ?? -10000}
          y={pos()?.y ?? 0}
          minWidth={minWidth() || undefined}
          flexDirection="column"
          paddingTop={theme.spacing.sm}
          paddingBottom={theme.spacing.sm}
        >
          <Surface
            role="overlay"
            elevation={styled().elevation ?? "overlay"}
            fill={theme.color.surface}
            radius={theme.radius.sm}
            material={styled().material}
            glow={styled().glow}
            outline={{ color: theme.color.border, width: theme.borderWidth.sm }}
          />
          <PopupList height={listHeight()} listRef={(n) => (list = n)} scrollRef={(s) => (scroll = s)}>
            <For each={props.options}>
              {(o: Option<T>) => <OptionRow option={o} padY={space("sm")} />}
            </For>
          </PopupList>
        </view>
      </view>,
    )
  }

  // Bottom sheet over a scrim; the content is a sibling of the scrim (Modal's
  // trick) so pressing an option never has the scrim on its hit path. The
  // rows scroll once the sheet would reach the clearance under the top safe
  // area; hidden until measured, as the dropdown is, so the full-height
  // list of the measuring frame is never seen.
  let Sheet = () => {
    let list: { id: number } | undefined
    let scroll: Scroll | undefined
    let [listHeight, setListHeight] = createSignal<number | null>(null)
    let reveal = createReveal()
    selectedRow = undefined
    // The padding above and below the rows: the sheet's height beyond the list's.
    let chrome = () => 2 * theme.spacing.md + env.safeArea.bottom
    onLayout(() => {
      let l = list && getBoundingBox(list)
      if (!l) return
      let room = env.windowSize.height - env.safeArea.top - sheetClearance() - chrome()
      let h = Math.min(l.height, room)
      if (h !== listHeight()) setListHeight(h)
      reveal(scroll, l, h)
    })
    let popScope: (() => void) | undefined
    onCleanup(() => popScope?.())
    return createPortal(
      <view
        ref={(n: { id: number }) => (popScope = pushNavScope(n))}
        position="absolute"
        top={0}
        left={0}
        right={0}
        bottom={0}
        opacity={listHeight() != null ? 1 : 0}
        transition={popupFadeOut()}
      >
        <view position="absolute" top={0} left={0} right={0} bottom={0} onPointerDown={() => setOpen(false)}>
          <d-rect transition={colorFade()} color={theme.color.scrim} />
        </view>
        <view
          position="absolute"
          left={0}
          right={0}
          bottom={0}
          flexDirection="column"
          paddingTop={theme.spacing.md}
          paddingBottom={theme.spacing.md + env.safeArea.bottom}
        >
          <Surface role="overlay" fill={theme.color.surface} radius={theme.radius.sm} material={styled().material} glow={null} />
          <PopupList height={listHeight()} listRef={(n) => (list = n)} scrollRef={(s) => (scroll = s)}>
            <For each={props.options}>
              {(o: Option<T>) => <OptionRow option={o} padY={Math.round(theme.spacing.md * 1.5)} />}
            </For>
          </PopupList>
        </view>
      </view>,
    )
  }

  let press = createPress({
    onPress: () => {
      setOpen(!open())
    },
  })
  // Theme-level per-component overrides merged under the instance style.
  let styled = () => ({ ...theme.components.select, ...props.style })
  let style = () => ({
    borderColor: theme.color.border,
    borderWidth: theme.borderWidth.sm,
    borderRadius: theme.radius.md,
    backgroundColor: theme.color.surface,
    ...theme.components.select,
    ...props.style,
    ...(press.focused() && policy.focusRing ? { borderWidth: theme.borderWidth.focus, borderColor: theme.color.ring } : {}),
  })
  // The chevron flips while the picker is open; travel motion, so it snaps
  // under reduced motion (the flip itself stays).
  let chevronSpin = () => {
    let t = travelMotion()
    return t && { rotate: t }
  }

  let split = () => splitTransition(props.transition)

  return (
    <view
      transition={split().root}
      onTransitionEnd={transitionEndFor("root", props.onTransitionEnd)}
      ref={(n: { id: number }) => {
        press.ref(n)
        trigger = n
      }}
      repaintBoundary
      flexDirection="row"
      alignItems="center"
      justifyContent="space-between"
      gap={theme.spacing.md}
      paddingTop={space("md")}
      paddingBottom={space("md")}
      paddingLeft={space("md")}
      paddingRight={space("md")}
      {...props.layout}
      x={style().x}
      y={style().y}
      scale={style().scale}
      rotate={style().rotate}
      opacity={style().opacity}
      {...press.handlers}
      cursor="pointer"
      focusable={!props.disabled}
      pointerEvents={props.disabled ? "none" : undefined}
    >
      <Surface
        role="control"
        elevation={style().elevation ?? "flat"}
        fill={style().backgroundColor ?? "transparent"}
        radius={style().borderRadius}
        material={style().material}
        glow={style().glow}
        pressed={press.pressed() && !props.disabled}
        hovered={press.hovered() && !props.disabled && policy.interaction !== "touch"}
        outline={(style().borderWidth ?? 0) > 0 ? { color: style().borderColor ?? "transparent", width: style().borderWidth! } : null}
        fillTransition={split().background}
        onFillTransitionEnd={transitionEndFor("background", props.onTransitionEnd)}
        outlineTransition={split().border}
        onOutlineTransitionEnd={transitionEndFor("border", props.onTransitionEnd)}
      />
      <Show
        when={selected()}
        fallback={<text transition={colorFade()} {...bodyText(theme.color.textMuted)}>{props.placeholder ?? ""}</text>}
      >
        <text transition={colorFade()} {...bodyText(props.disabled ? theme.color.textMuted : theme.color.text)}>{selected()!.label}</text>
      </Show>
      <view rotate={open() ? Math.PI : 0} transition={chevronSpin()}>
        <Show
          when={theme.icons.chevronDown}
          fallback={
            <view width={12} height={8}>
              <d-path
                d="M 2 2 L 6 6 L 10 2"
                drawStyle="stroke"
                transition={colorFade()}
                color={theme.color.textMuted}
                strokeWidth={2}
                strokeCap="round"
                strokeJoin="round"
              />
            </view>
          }
        >
          <Icon src={theme.icons.chevronDown!} size={12} color={theme.color.textMuted} />
        </Show>
      </view>
      <Show when={open()}>
        <Show when={policy.interaction === "touch"} fallback={<Dropdown />}>
          <Sheet />
        </Show>
      </Show>
    </view>
  )
}
