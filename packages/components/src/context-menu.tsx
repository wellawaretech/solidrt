import { createEffect, createSignal, createLongPress, createPortal, onBack, onLayout, getBoundingBox, Show, For, env } from "@solidrt/core"
import type { LayoutProps, PointerEvent } from "@solidrt/core"
import { createPress } from "./press"
import { placePopup, PopupList } from "./popup"
import { theme } from "./theme"
import { policy } from "./policy"
import { space } from "./spacing"
import { typeStyle } from "./typography"
import type { TransitionProps } from "./types"
import { splitTransition, transitionEndFor } from "./types"
import { colorFade, popupFadeOut, PressFeedback } from "./motion"
import { Surface } from "./surface"

export interface ContextMenuItem {
  label: string
  onSelect?: () => void
  disabled?: boolean
}

export interface ContextMenuProps extends TransitionProps {
  items: ContextMenuItem[]
  // The content the menu attaches to.
  children?: any
  layout?: LayoutProps
}

// Scrim kept visible above a bottom sheet at its tallest.
let sheetClearance = () => theme.spacing.xl

/**
 * Secondary actions on the wrapped content. The opening gesture follows the
 * physical pointer: right-click for a mouse, long-press for touch and pen
 * (core's long-press recognizer: it wins the finger at its timer, so a
 * pressable inside retracts and never fires, and a scroll that started
 * first keeps it). The presentation forks on the interaction policy: touch
 * gets a bottom sheet over a scrim, desktop/hybrid an anchored menu at the
 * pointer, on the side with room. A list taller than the room scrolls.
 * Pressing outside, Escape and the user's back close without selecting.
 */
export function ContextMenu(props: ContextMenuProps) {
  let [open, setOpen] = createSignal(false)
  let [point, setPoint] = createSignal({ x: 0, y: 0 })

  let openAt = (x: number, y: number) => {
    setPoint({ x, y })
    setOpen(true)
  }
  let longPress = createLongPress({ onLongPress: at => openAt(at.clientX, at.clientY) })

  let handleDown = (e: PointerEvent) => {
    if (e.button === 2) openAt(e.clientX, e.clientY)
    else if (e.pointerType !== "mouse") longPress.handlers.onPointerDown(e)
  }

  let choose = (item: ContextMenuItem) => {
    setOpen(false)
    item.onSelect?.()
  }

  // Open, the menu is one step of core's back stack: Escape and the user's
  // back close it as a press outside does, selecting nothing.
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

  let ItemRow = (p: { item: ContextMenuItem; padY: number }) => {
    let press = createPress({ onPress: () => choose(p.item) })
    return (
      <view
        ref={press.ref}
        repaintBoundary
        flexDirection="row"
        alignItems="center"
        paddingTop={p.padY}
        paddingBottom={p.padY}
        paddingLeft={space("md")}
        paddingRight={space("md")}
        {...press.handlers}
        cursor="pointer"
        pointerEvents={p.item.disabled ? "none" : undefined}
      >
        <PressFeedback
          pressed={press.pressed()}
          hovered={press.hovered() && policy.interaction !== "touch"}
        />
        <text transition={colorFade()} {...bodyText(p.item.disabled ? theme.color.textMuted : theme.color.text)}>{p.item.label}</text>
      </view>
    )
  }

  // Anchored at the opening pointer position, on the side with room
  // (placePopup, a point anchor with no gap). Same reflow-free placement as
  // Tooltip/Select: portal at the window root, measured in onLayout, moved
  // with x/y paint transforms. The rows scroll once taller than the room.
  // An overlay face (the sheet sits on a scrim, so it stays flat).
  let Menu = () => {
    let menu: { id: number } | undefined
    let list: { id: number } | undefined
    let [pos, setPos] = createSignal<{ x: number; y: number } | null>(null)
    let [listHeight, setListHeight] = createSignal<number | null>(null)
    // The padding above and below the rows: the menu's height beyond the list's.
    let chrome = () => 2 * theme.spacing.sm
    onLayout(() => {
      let b = menu && getBoundingBox(menu)
      let l = list && getBoundingBox(list)
      if (!b || !l) return
      let at = point()
      let p = placePopup({ x: at.x, y: at.y, width: 0, height: 0 }, { width: b.width, height: l.height + chrome() }, { gap: 0 })
      let h = Math.min(l.height, p.room - chrome())
      if (h !== listHeight()) setListHeight(h)
      let cur = pos()
      if (!cur || cur.x !== p.x || cur.y !== p.y) setPos({ x: p.x, y: p.y })
    })
    // The fade is driven by the position write, not a mount-time `from`: the
    // menu parks offscreen until measured, so a `from` would play unseen.
    // The exit fade lives on this root (the node the close unmounts).
    return createPortal(
      <view
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
          minWidth={theme.size.menuMinWidth}
          flexDirection="column"
          paddingTop={theme.spacing.sm}
          paddingBottom={theme.spacing.sm}
        >
          <Surface
            role="overlay"
            elevation={theme.components.contextMenu?.elevation ?? "overlay"}
            fill={theme.components.contextMenu?.backgroundColor ?? theme.color.surface}
            radius={theme.components.contextMenu?.borderRadius ?? theme.radius.sm}
            material={theme.components.contextMenu?.material}
            glow={theme.components.contextMenu?.glow}
            outline={{
              color: theme.components.contextMenu?.borderColor ?? theme.color.border,
              width: theme.components.contextMenu?.borderWidth ?? theme.borderWidth.sm,
            }}
          />
          <PopupList height={listHeight()} listRef={(n) => (list = n)}>
            <For each={props.items}>
              {(item: ContextMenuItem) => <ItemRow item={item} padY={space("sm")} />}
            </For>
          </PopupList>
        </view>
      </view>,
    )
  }

  // Bottom sheet over a scrim; content is a sibling of the scrim (Modal's
  // trick) so a row press never has the scrim on its hit path. The rows
  // scroll once the sheet would reach the clearance under the top safe
  // area; hidden until measured, as the menu is, so the full-height list
  // of the measuring frame is never seen.
  let Sheet = () => {
    let list: { id: number } | undefined
    let [listHeight, setListHeight] = createSignal<number | null>(null)
    // The padding above and below the rows: the sheet's height beyond the list's.
    let chrome = () => 2 * theme.spacing.md + env.safeArea.bottom
    onLayout(() => {
      let l = list && getBoundingBox(list)
      if (!l) return
      let room = env.windowSize.height - env.safeArea.top - sheetClearance() - chrome()
      let h = Math.min(l.height, room)
      if (h !== listHeight()) setListHeight(h)
    })
    return createPortal(
      <view
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
          <Surface
            role="overlay"
            fill={theme.components.contextMenu?.backgroundColor ?? theme.color.surface}
            radius={theme.components.contextMenu?.borderRadius ?? theme.radius.sm}
            material={theme.components.contextMenu?.material}
            glow={null}
          />
          <PopupList height={listHeight()} listRef={(n) => (list = n)}>
            <For each={props.items}>
              {(item: ContextMenuItem) => <ItemRow item={item} padY={Math.round(theme.spacing.md * 1.5)} />}
            </For>
          </PopupList>
        </view>
      </view>,
    )
  }

  return (
    <view
      transition={splitTransition(props.transition).root}
      onTransitionEnd={transitionEndFor("root", props.onTransitionEnd)}
      onPointerDown={handleDown}
      onPointerMove={longPress.handlers.onPointerMove}
      onPointerUp={longPress.handlers.onPointerUp}
      onPointerCancel={longPress.handlers.onPointerCancel}
      {...props.layout}
    >
      {props.children}
      <Show when={open()}>
        <Show when={policy.interaction === "touch"} fallback={<Menu />}>
          <Sheet />
        </Show>
      </Show>
    </view>
  )
}
