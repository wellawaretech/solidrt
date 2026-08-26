import { createSignal, onCleanup, createPortal, onLayout, getBoundingBox, Show, For, env } from "@solidrt/core"
import type { LayoutProps, PointerEvent } from "@solidrt/core"
import { createPress } from "./press"
import { theme } from "./theme"
import { policy } from "./policy"
import { space } from "./spacing"
import { typeStyle } from "./typography"
import type { TransitionProps } from "./types"
import { splitTransition, transitionEndFor } from "./types"

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

const LONG_PRESS_MS = 500
// Finger travel (window px) that cancels a pending long-press.
const MOVE_SLOP = 8
// Minimum distance kept between the menu and the window edges.
let margin = () => theme.spacing.sm

/**
 * Secondary actions on the wrapped content. The opening gesture follows the
 * physical pointer: right-click for a mouse, long-press for touch. The
 * presentation forks on the interaction policy: touch gets a bottom sheet over
 * a scrim, desktop/hybrid an anchored menu at the pointer. Pressing outside
 * closes without selecting.
 */
export function ContextMenu(props: ContextMenuProps) {
  let [open, setOpen] = createSignal(false)
  let [point, setPoint] = createSignal({ x: 0, y: 0 })
  let timer: ReturnType<typeof setTimeout> | undefined
  let downAt: { x: number; y: number } | undefined

  let openAt = (x: number, y: number) => {
    setPoint({ x, y })
    setOpen(true)
  }
  let cancelHold = () => {
    clearTimeout(timer)
    downAt = undefined
  }
  onCleanup(cancelHold)

  let handleDown = (e: PointerEvent) => {
    if (e.button === 2) {
      cancelHold()
      openAt(e.clientX, e.clientY)
    } else if (e.pointerType === "touch") {
      downAt = { x: e.clientX, y: e.clientY }
      clearTimeout(timer)
      timer = setTimeout(() => {
        if (downAt) openAt(downAt.x, downAt.y)
        downAt = undefined
      }, LONG_PRESS_MS)
    }
  }
  let handleMove = (e: PointerEvent) => {
    if (!downAt) return
    if (Math.abs(e.clientX - downAt.x) > MOVE_SLOP || Math.abs(e.clientY - downAt.y) > MOVE_SLOP) cancelHold()
  }

  let choose = (item: ContextMenuItem) => {
    setOpen(false)
    item.onSelect?.()
  }

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
        pointerEvents={p.item.disabled ? "none" : undefined}
      >
        <d-rect
          color={
            press.pressed()
              ? theme.color.overlayPressed
              : press.hovered() && policy.interaction !== "touch"
                ? theme.color.overlayHover
                : "transparent"
          }
        />
        <text {...bodyText(p.item.disabled ? theme.color.textMuted : theme.color.text)}>{p.item.label}</text>
      </view>
    )
  }

  // Anchored at the opening pointer position, flipping up when it would run
  // off the bottom. Same reflow-free placement as Tooltip/Select: portal at
  // the window root, measured in onLayout, moved with x/y paint transforms.
  let Menu = () => {
    let menu: { id: number } | undefined
    let [pos, setPos] = createSignal<{ x: number; y: number } | null>(null)
    onLayout(() => {
      let b = menu && getBoundingBox(menu)
      if (!b) return
      let p = point()
      let x = Math.round(Math.min(Math.max(p.x, margin()), env.windowSize.width - b.width - margin()))
      let y = Math.round(
        Math.max(p.y + b.height > env.windowSize.height - margin() ? p.y - b.height : p.y, margin()),
      )
      let cur = pos()
      if (!cur || cur.x !== x || cur.y !== y) setPos({ x, y })
    })
    return createPortal(
      <view position="absolute" top={0} left={0} right={0} bottom={0}>
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
          <d-rect color={theme.color.surface} radius={theme.radius.sm} />
          <For each={props.items}>
            {(item: ContextMenuItem) => <ItemRow item={item} padY={space("sm")} />}
          </For>
          <d-rect
            drawStyle="stroke"
            color={theme.color.border}
            strokeWidth={theme.borderWidth.sm}
            radius={theme.radius.sm}
          />
        </view>
      </view>,
    )
  }

  // Bottom sheet over a scrim; content is a sibling of the scrim (Modal's
  // trick) so a row press never has the scrim on its hit path.
  let Sheet = () =>
    createPortal(
      <view position="absolute" top={0} left={0} right={0} bottom={0}>
        <view position="absolute" top={0} left={0} right={0} bottom={0} onPointerDown={() => setOpen(false)}>
          <d-rect color={theme.color.scrim} />
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
          <d-rect color={theme.color.surface} radius={theme.radius.sm} />
          <For each={props.items}>
            {(item: ContextMenuItem) => <ItemRow item={item} padY={Math.round(theme.spacing.md * 1.5)} />}
          </For>
        </view>
      </view>,
    )

  return (
    <view
      transition={splitTransition(props.transition).root}
      onTransitionEnd={transitionEndFor("root", props.onTransitionEnd)}
      onPointerDown={handleDown}
      onPointerMove={handleMove}
      onPointerUp={cancelHold}
      onPointerLeave={cancelHold}
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
