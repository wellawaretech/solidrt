// An app entry as the runtime starts one: its top level renders. Loaded by
// app.test.tsx through `app.load`.

import { render } from "../../src/index.ts"

render(() => (
  <window>
    <text>from the entry</text>
  </window>
))
