import { createMemo } from "@solidjs/signals"
import type { LayoutProps } from "@solidrt/core"
import qrcode from "qrcode-generator"

export interface QrCodeProps {
  // The string to encode (URL, pairing ticket, text, ...).
  data: string
  // Pixels per QR module (the smallest square). The grid is
  // moduleCount * moduleSize on a side, plus the margin around it.
  moduleSize?: number
  // Quiet-zone padding in pixels around the grid. Scanners need a light border,
  // so keep this non-zero.
  margin?: number
  // Dark/light module colors. Defaults are black on white for reliable scanning
  // regardless of theme; override only if you know the contrast still holds.
  color?: string
  background?: string
  // Error-correction level: higher tolerates more damage but packs denser and
  // caps the data length sooner.
  level?: "L" | "M" | "Q" | "H"
  // Corner radius of the background panel.
  radius?: number
  layout?: LayoutProps
}

const MODULE_SIZE = 6
const MARGIN = 16
const RADIUS = 8

// Render a QR for `data` as primitives: merge horizontal runs of dark modules
// per row into a single d-rect, placed at explicit coordinates on a light
// quiet-zone panel. Everything inside the panel is detached, so a data change
// repaints without touching layout; the panel view itself has a fixed size
// (module count * module size + margins) that only changes when the data
// crosses a QR version boundary.
export function QrCode(props: QrCodeProps) {
  let grid = createMemo(() => {
    let qr = qrcode(0, props.level ?? "M")
    qr.addData(props.data)
    qr.make()
    let n = qr.getModuleCount()

    let runs: { x: number; y: number; len: number }[] = []
    for (let y = 0; y < n; y++) {
      let x = 0
      while (x < n) {
        if (!qr.isDark(y, x)) {
          x++
          continue
        }
        let len = 1
        while (x + len < n && qr.isDark(y, x + len)) len++
        runs.push({ x, y, len })
        x += len
      }
    }
    return { n, runs }
  })

  let size = () => props.moduleSize ?? MODULE_SIZE
  let margin = () => props.margin ?? MARGIN
  let side = () => grid().n * size() + 2 * margin()

  return (
    <view repaintBoundary width={side()} height={side()} {...props.layout}>
      <d-rect color={props.background ?? "#ffffff"} radius={props.radius ?? RADIUS} />
      {grid().runs.map((run) => (
        <d-rect
          x={margin() + run.x * size()}
          y={margin() + run.y * size()}
          w={run.len * size()}
          h={size()}
          color={props.color ?? "#000000"}
        />
      ))}
    </view>
  )
}
