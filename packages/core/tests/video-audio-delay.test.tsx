// The `audioDelay` option of flux:video's open: seconds the sound's output
// path delays the sound, the same on both players. A negative or
// non-finite value throws at the call; a valid one is taken, and the open
// goes on to the source (here a missing file, which rejects).

import { test, expect } from "@solidrt/test"
import { open } from "flux:video"

// A path no fixture has: the open reads it and rejects.
const MISSING = "no-such-clip.webm"
// An external speaker's delay, distinct from the default.
const SPEAKER_DELAY_S = 0.2

test("audioDelay rejects a negative or non-finite value and takes a valid one", async app => {
  await app.mount(() => <view />)
  for (let audioDelay of [-0.01, Number.NaN, Number.POSITIVE_INFINITY]) {
    expect(() => open(MISSING, { audioDelay })).toThrow(/audioDelay must be a non-negative number of seconds/)
  }
  await expect(open(MISSING, { audioDelay: SPEAKER_DELAY_S })).rejects.toMatchObject({ kind: "network" })
  await expect(open(MISSING, { audioDelay: 0 })).rejects.toMatchObject({ kind: "network" })
})
