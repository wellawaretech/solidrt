import { Show } from "@solidrt/core"
import type { LayoutProps } from "@solidrt/core"
import { policy } from "./policy"

export interface SplitViewProps {
  // The list (or primary) pane.
  list?: any
  // The detail (or secondary) pane.
  detail?: any
  // Single-pane mode only: show the detail instead of the list. The app owns
  // this navigation state; two-pane mode ignores it.
  showDetail?: boolean
  // Width of the list pane in two-pane mode.
  listWidth?: number
  layout?: LayoutProps
}

const LIST_WIDTH = 320

/**
 * A list-detail container driven by the layout policy: two-pane shows the list
 * beside the detail, single-pane shows one pane at a time per `showDetail`.
 * Keep pane state (selection, scroll) in the app, not in the panes: crossing
 * a breakpoint re-arranges and can remount them.
 * SplitView draws no chrome; a back affordance in the single-pane detail is
 * the app's to render (fork on policy.layout, as the shell example does).
 *
 * Panes get no padding, max-width or alignment either - that is content, and
 * SplitView cannot know the intended reading width. Give the single-pane
 * detail the same treatment as the list (centered max-width column), otherwise
 * crossing the breakpoint leaves the detail hugging the window's left edge
 * while every other screen stays centered. Two-pane wants the opposite: the
 * detail sits against the pane edge, since the pane already bounds its width.
 */
export function SplitView(props: SplitViewProps) {
  return (
    <Show
      when={policy.layout === "twoPane"}
      fallback={
        <view flexDirection="column" {...props.layout}>
          <Show when={props.showDetail} fallback={props.list}>
            {props.detail}
          </Show>
        </view>
      }
    >
      <view flexDirection="row" {...props.layout}>
        <view width={props.listWidth ?? LIST_WIDTH} flexDirection="column">
          {props.list}
        </view>
        <view flex={1} flexDirection="column">
          {props.detail}
        </view>
      </view>
    </Show>
  )
}
