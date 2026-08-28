mod common;

use common::run_source;

// Both calls default to premultiplied alpha, so the pixels handed in must be
// valid premultiplied ones (no channel above its alpha) to survive the trip.
#[tokio::test]
async fn png_round_trips_premultiplied_pixels() {
  let captured = run_source(
    r#"
      import { decodeImage, encodeImage } from "flux:image"
      let pixels = new Uint8Array([255, 0, 0, 255, 0, 128, 0, 128])
      let png = encodeImage({ data: pixels, width: 2, height: 1 })
      let back = decodeImage(png)
      console.log(`${back.width}x${back.height} ${Array.from(back.data).join(",")}`)
    "#,
  )
  .await;
  assert_eq!(captured.log(), "2x1 255,0,0,255,0,128,0,128");
}

// Straight alpha on both ends is what a PNG file stores, so it is byte-exact.
#[tokio::test]
async fn png_round_trips_straight_pixels() {
  let captured = run_source(
    r#"
      import { decodeImage, encodeImage } from "flux:image"
      let pixels = new Uint8Array([255, 0, 0, 255, 0, 255, 0, 128])
      let png = encodeImage({ data: pixels, width: 2, height: 1 }, { alpha: "straight" })
      let back = decodeImage(png, { alpha: "straight" })
      console.log(`${back.width}x${back.height} ${Array.from(back.data).join(",")}`)
    "#,
  )
  .await;
  assert_eq!(captured.log(), "2x1 255,0,0,255,0,255,0,128");
}

#[tokio::test]
async fn jpeg_decodes_opaque_with_same_dims() {
  let captured = run_source(
    r#"
      import { decodeImage, encodeImage } from "flux:image"
      let pixels = new Uint8Array([255, 0, 0, 255, 0, 255, 0, 128])
      // An explicit undefined options argument is normal JS for "no options".
      let png = encodeImage({ data: pixels, width: 2, height: 1 }, undefined)
      let jpg = encodeImage(decodeImage(png), { format: "jpeg", quality: 0.8 })
      let back = decodeImage(jpg)
      console.log(`${back.width}x${back.height} len=${back.data.length} a=${back.data[3]},${back.data[7]}`)
    "#,
  )
  .await;
  assert_eq!(captured.log(), "2x1 len=8 a=255,255");
}

#[tokio::test]
async fn bad_input_throws() {
  let captured = run_source(
    r#"
      import { decodeImage, encodeImage } from "flux:image"
      let img = { data: new Uint8Array(8), width: 2, height: 1 }
      for (let f of [
        () => decodeImage(new Uint8Array([1, 2, 3])),
        () => encodeImage({ data: new Uint8Array(8), width: 3, height: 1 }),
        () => encodeImage(img, { format: "tiff" }),
        () => encodeImage(img, { quality: 1.5 }),
        () => encodeImage({ data: "nope", width: 2, height: 1 }),
      ]) {
        try {
          f()
          console.log("no throw")
        } catch (e) {
          console.log(e.message)
        }
      }
    "#,
  )
  .await;
  let lines = captured.log();
  let mut it = lines.lines();
  assert!(it.next().expect("decode line").starts_with("decodeImage:"), "log: {lines}");
  assert!(it.next().expect("length line").contains("3x1"), "log: {lines}");
  assert!(it.next().expect("format line").contains("unknown format \"tiff\""), "log: {lines}");
  assert!(it.next().expect("quality line").contains("out of range"), "log: {lines}");
  assert!(it.next().expect("data line").contains("img.data"), "log: {lines}");
}

#[tokio::test]
async fn image_capability_is_listed() {
  let captured = run_source(r#"console.log(Flux.capabilities.includes("image"))"#).await;
  assert_eq!(captured.log(), "true");
}
