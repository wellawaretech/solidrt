// The QR scan screen: a full-window camera with center cover-crop and a
// corner-bracket reticle, feeding scanned data back out to dial. Dismissal is a
// close (X) on a scrim disc, not the heading-row back arrow the other screens
// use: there is no heading to hang it off, and leaving a live camera reads as
// closing a viewfinder rather than stepping back up a hierarchy.
import { env, createEffect, untrack } from "@solidrt/core"
import { createCamera, type BarcodeResult } from "@solidrt/core/camera"
import { Show } from "solid-js"
import { View, Pressable, Icon, SafeArea, space, type PressState } from "@solidrt/components"
import { TAP_TARGET } from "./types"

// The scan reticle's stroke thickness and corner radius (logical px). The
// bracket paths are inset by half the stroke so the round caps stay inside
// the reticle box; each corner turns through an arc so the bend itself is
// rounded, not just the stroke join.
const RETICLE_STROKE = 10
const RETICLE_RADIUS = 20

// Lucide x, stroked with currentColor so Icon recolors it.
const CLOSE_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg>`

// The close button's scrim. Fixed, not themed: it sits on the camera feed, not
// on a themed surface, and has to stay legible over whatever is in frame.
const SCRIM = "rgba(0, 0, 0, 0.45)"
const SCRIM_HOVER = "rgba(0, 0, 0, 0.65)"

// Full-window camera with center cover-crop and a corner-bracket scan reticle.
// Mounted only while scanning (under <Match>), so the camera opens with the
// screen and closes when it leaves. The camera, reticle, and controls are
// absolutely positioned layers: in flow they would stack in the column and
// push each other off-center.
export function ScanScreen(props: {
  onScanned: (data: string) => void
  onCancel: () => void
  onError: (message: string) => void
}) {
  let cam = createCamera(untrack(() => ({ scan: ["qr"] as "qr"[] })))
  createEffect(
    () => cam.barcode(),
    (b?: BarcodeResult) => {
      if (b) props.onScanned(b.data)
    },
  )
  createEffect(
    () => cam.error(),
    (e?: Error) => {
      if (e) props.onError(e.message)
    },
  )

  // Source rect for object-fit: cover, centered, in camera pixels.
  let crop = () => {
    let cw = cam.width()
    let ch = cam.height()
    let { width: w, height: h } = env.windowSize
    if (!cw || !ch || !w || !h) return null
    let scale = Math.max(w / cw, h / ch)
    let srcW = w / scale
    let srcH = h / scale
    return { w, h, srcX: (cw - srcW) / 2, srcY: (ch - srcH) / 2, srcW, srcH }
  }

  let reticle = () => {
    let { width: w, height: h } = env.windowSize
    let s = Math.round(Math.min(w, h) * 0.55)
    let l = Math.round(s * 0.18)
    let i = RETICLE_STROKE / 2
    let r = RETICLE_RADIUS
    return {
      size: s,
      d:
        `M${i} ${l} L${i} ${i + r} A ${r} ${r} 0 0 1 ${i + r} ${i} L${l} ${i} ` +
        `M${s - l} ${i} L${s - i - r} ${i} A ${r} ${r} 0 0 1 ${s - i} ${i + r} L${s - i} ${l} ` +
        `M${s - i} ${s - l} L${s - i} ${s - i - r} A ${r} ${r} 0 0 1 ${s - i - r} ${s - i} L${s - l} ${s - i} ` +
        `M${l} ${s - i} L${i + r} ${s - i} A ${r} ${r} 0 0 1 ${i} ${s - i - r} L${i} ${s - l}`,
    }
  }

  return (
    <View layout={{ flexGrow: 1, position: "relative" }} style={{ backgroundColor: "black" }}>
      <Show when={cam.texture() != null && crop()}>
        {(c) => (
          <texture
            position="absolute"
            src={cam.texture()}
            w={c().w}
            h={c().h}
            srcX={c().srcX}
            srcY={c().srcY}
            srcW={c().srcW}
            srcH={c().srcH}
          />
        )}
      </Show>
      <View
        layout={{
          position: "absolute",
          width: "100%",
          height: "100%",
          justifyContent: "center",
          alignItems: "center",
        }}
      >
        <View layout={{ width: reticle().size, height: reticle().size }}>
          <d-path
            d={reticle().d}
            color="white"
            drawStyle="stroke"
            strokeWidth={RETICLE_STROKE}
            strokeCap="round"
            strokeJoin="round"
          />
        </View>
      </View>
      <View layout={{ position: "absolute", width: "100%", height: "100%" }}>
        <SafeArea>
          <View layout={{ flexGrow: 1, padding: space("xl") }}>
            <View layout={{ flexDirection: "row" }}>
              <Pressable
                onPress={props.onCancel}
                layout={{
                  width: TAP_TARGET,
                  height: TAP_TARGET,
                  alignItems: "center",
                  justifyContent: "center",
                }}
                style={(s: PressState) => ({
                  backgroundColor: s.hovered ? SCRIM_HOVER : SCRIM,
                  borderRadius: TAP_TARGET / 2,
                })}
              >
                <Icon src={CLOSE_SVG} size={22} color="white" />
              </Pressable>
            </View>
          </View>
        </SafeArea>
      </View>
    </View>
  )
}
