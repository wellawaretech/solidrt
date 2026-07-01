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

// Render a QR for `data` as primitives: merge horizontal runs of same-color
// modules per row into a single sized box, on a light quiet-zone panel. The
// module grid recomputes only when the data or error-correction level changes.
export function QrCode(props: QrCodeProps) {
  let rows = createMemo(() => {
    let qr = qrcode(0, props.level ?? "M")
    qr.addData(props.data)
    qr.make()
    let n = qr.getModuleCount()

    let out: { dark: boolean; len: number }[][] = []
    for (let y = 0; y < n; y++) {
      let runs: { dark: boolean; len: number }[] = []
      let x = 0
      while (x < n) {
        let dark = qr.isDark(y, x)
        let len = 1
        while (x + len < n && qr.isDark(y, x + len) === dark) len++
        runs.push({ dark, len })
        x += len
      }
      out.push(runs)
    }
    return out
  })

  let size = () => props.moduleSize ?? MODULE_SIZE

  return (
    <view flexDirection="column" padding={props.margin ?? MARGIN} {...props.layout}>
      <d-rect color={props.background ?? "#ffffff"} radius={props.radius ?? RADIUS} />
      {rows().map((runs) => (
        <view flexDirection="row">
          {runs.map((run) => (
            <view width={run.len * size()} height={size()}>
              {run.dark ? <d-rect color={props.color ?? "#000000"} /> : null}
            </view>
          ))}
        </view>
      ))}
    </view>
  )
}
