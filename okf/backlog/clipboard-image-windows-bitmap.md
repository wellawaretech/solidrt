---
title: Check the clipboard image on Windows and macOS
description: "The clipboard image read falls back to a Windows bitmap (CF_DIB, read as image/bmp and converted to PNG on the JS thread) but has only been compiled, never run on Windows; a write offers only the registered PNG format, so a Windows app that pastes only CF_DIB gets nothing from us; and a macOS app that copies TIFF only likely reads as no image. None of it run yet."
created: 2026-11-11
---

# Check the clipboard image on Windows and macOS

SDL writes `image/png` on Windows as the registered "PNG" clipboard
format, and reads `image/png` only from it. Many Windows sources put only
a bitmap (`CF_DIB` / `CF_DIBV5`) there. The image clipboard
([clipboard-image](../done/clipboard-image.md)) handles the read side: ore
falls back to `image/bmp`, which SDL hands over as a BMP file, and the flux
plugin converts it to PNG through forge's codec. That fallback is compiled
in CI but has not run on a Windows machine.

Two more open points, neither verified:

- macOS: SDL reads `image/png` as the pasteboard's PNG type only
  (`Cocoa_GetClipboardData` asks for the UTI of the MIME type), and many
  macOS apps copy an image as TIFF. Such a copy likely reads as no image,
  and forge has no TIFF decoder to convert it.
- The bitmap conversion (decode the BMP, encode a PNG) runs on the JS
  thread, inline as `flux:image` runs its codec. A full-screen screenshot
  is tens of megabytes of pixels (a 4K screen is about 33 MB of RGBA), so
  the frame it lands in may stall for a noticeable time; measure it on
  Windows before deciding whether it moves off the thread.

## What done looks like

- On a Windows machine, `navigator.clipboard.read()` returns one
  `image/png` item after a copy from: the PrintScreen key, an image
  editor (Paint), and an office suite (an image copied from a document).
  `decodeImage` gives the copied size and pixels.
- The conversion of a full-screen screenshot timed on Windows (release
  build), and moved off the JS thread if it stalls a frame noticeably.
- On a Mac, copies from common apps tried (an image viewer, a browser,
  a screenshot to the clipboard) to find one that offers TIFF only; if
  one reads as no image, a decision on asking for TIFF too, which needs a
  TIFF decode.
- A decision on the write side, with its reason. The option: a write also
  offers `image/bmp` (SDL carries it as `CF_DIB` in the same call), so an
  app that pastes only `CF_DIB` gets the image. It needs a BMP encode in
  forge (`forge::image` decodes BMP but encodes only PNG and JPEG) and an
  uncompressed copy of the image in memory per write,
  beside the PNG, until the clipboard is replaced. Check first which
  common apps paste only `CF_DIB`.

## What it involves

- A Windows build of the dev client and a scratch app that writes and
  reads an image through debug commands, as the Linux check did.
- For the write side, if taken: `ore::clipboard::sdl::set_data` offering
  two types (the data callback answers per MIME type), and the BMP bytes
  made in the flux plugin from the PNG, so ore stays codec-free.
