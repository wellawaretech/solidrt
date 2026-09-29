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

// Compressed textures: an 8x8 image encodes to a KTX2 with its four-level
// chain, and transcodes to the block formats (7 blocks of 16 bytes) and to
// rgba8 (85 pixels) with the sRGB flag carried by the file.
#[cfg(feature = "ktx2")]
#[tokio::test]
async fn texture_encodes_and_transcodes() {
  let captured = run_source(
    r#"
      import { encodeTexture, transcodeTexture } from "flux:image"
      let data = new Uint8Array(8 * 8 * 4)
      for (let i = 0; i < data.length; i += 4) data.set([200, 100, 50, 255], i)
      let file = await encodeTexture({ data, width: 8, height: 8 }, { codec: "etc1s", srgb: true, mipmap: true })
      let magic = Array.from(file.slice(1, 7)).map(c => String.fromCharCode(c)).join("")
      let lines = [magic]
      for (let target of ["etc2-rgba8", "bc7-rgba8", "rgba8"]) {
        let t = await transcodeTexture(file, { target })
        lines.push(`${t.format} ${t.width}x${t.height} mipmap=${t.mipmap} ${t.data.length} ${t.data instanceof Uint8Array}`)
      }
      console.log(lines.join(" | "))
    "#,
  )
  .await;
  assert_eq!(
    captured.log(),
    "KTX 20 | etc2-rgba8-srgb 8x8 mipmap=true 112 true | bc7-rgba8-srgb 8x8 mipmap=true 112 true | rgba8-srgb 8x8 mipmap=true 340 true"
  );
}

// A headless runtime has no GPU to pick a target for, a file that is not a
// KTX2 rejects, and a missing codec throws at the call.
#[cfg(feature = "ktx2")]
#[tokio::test]
async fn texture_calls_refuse_bad_input() {
  let captured = run_source(
    r#"
      import { encodeTexture, transcodeTexture } from "flux:image"
      let out = []
      let attempt = async (name, fn) => {
        try { await fn(); out.push(name + ": NO THROW") } catch (e) { out.push(name + ": " + e.message) }
      }
      let png = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a])
      await attempt("no target", () => transcodeTexture(png))
      await attempt("not ktx2", () => transcodeTexture(png, { target: "etc2-rgba8" }))
      await attempt("bad target", () => transcodeTexture(png, { target: "astc" }))
      await attempt("no codec", () => encodeTexture({ data: new Uint8Array(4), width: 1, height: 1 }, {}))
      await attempt("short", () => encodeTexture({ data: new Uint8Array(4), width: 2, height: 2 }, { codec: "uastc" }))
      console.log(out.join("\n"))
    "#,
  )
  .await;
  let log = captured.log();
  assert!(log.contains("no target: transcodeTexture: this runtime has no GPU"), "{log}");
  assert!(log.contains("not ktx2: transcodeTexture: not a KTX2 file"), "{log}");
  assert!(log.contains("bad target: transcodeTexture: unknown target 'astc'"), "{log}");
  assert!(log.contains("no codec: encodeTexture: codec is required"), "{log}");
  assert!(log.contains("short: encodeTexture: expected 16 bytes"), "{log}");
}
