import qrcode from "qrcode-generator"

// Render `text` as a terminal QR code: a white tile with black modules
// (explicit ANSI colors) plus the quiet zone the QR spec requires. Drawing with
// the terminal's default foreground inverts the code on dark themes, which
// standard decoders reject.
export function printQr(text: string) {
  let qr = qrcode(0, "L")
  qr.addData(text)
  qr.make()
  let modCount = qr.getModuleCount()
  const QR_INK = "\x1b[30;107m" // black modules on bright-white tile (tile = background)
  const QR_TILE_FG = "\x1b[97m" // bright-white as foreground over the default background
  const QR_RESET = "\x1b[0m"
  const QUIET_ZONE = 2 // modules (spec says 4, but scanners cope and it reads tighter)
  let qrWidth = modCount + 2 * QUIET_ZONE
  let dark = (y: number, x: number) => y >= 0 && y < modCount && x >= 0 && x < modCount && qr.isDark(y, x)
  // modCount is always odd, so the tile is a half-line taller than an even row
  // count. The loop packs two module-rows per line via half-blocks and stops on
  // the last content row, leaving the bottom quiet zone half a line short of the
  // full-line top quiet zone.
  for (let y = -QUIET_ZONE; y < modCount + QUIET_ZONE - 1; y += 2) {
    let row = "  " + QR_INK
    for (let x = -QUIET_ZONE; x < modCount + QUIET_ZONE; x++) {
      let top = dark(y, x)
      let bot = dark(y + 1, x)
      row += top && bot ? "\u2588" : top ? "\u2580" : bot ? "\u2584" : " "
    }
    console.log(row + QR_RESET)
  }
  // Close that gap with a half-height tile line: upper half painted in the tile
  // color (foreground), lower half the terminal background. The 0.5 here plus
  // the 0.5 already under the last content row equal the full-line top margin.
  console.log("  " + QR_TILE_FG + "\u2580".repeat(qrWidth) + QR_RESET)
}
