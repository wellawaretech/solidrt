---
title: Pin the cursor prop decode in flux's property tests
description: The router side of the cursor prop is covered in alloy, but the flux decode (21 accepted names, the handle form, the two rejection messages) has no test in the existing apply_jsx harness.
created: 2026-09-22
---

# Pin the cursor prop decode in flux's property tests

## Symptom

`cursor` decodes in `flux/src/alloy_plugins/properties/mod.rs`
(`cursor_by_name`, `cursor_handle`): 20 CSS names to `Cursor::System`,
`none` to `Cursor::Hidden`, a non-negative integer to `Cursor::Custom`,
null clears, and anything else returns an Err naming the value. None of
that is pinned; a renamed keyword or a dropped arm would only show up
in an app.

## What done looks like

A test in `flux/src/tests/properties.rs`, through the existing `apply`
helper (it drives `apply_jsx` the way the FFI does): every accepted name
lands on the expected `HitConfig.cursor`, `"none"` is `Hidden`, a number
is `Custom`, a negative or fractional number and an unknown name return
the messages that name the value and the accepted set, and null clears.
Damage is `None` throughout.
