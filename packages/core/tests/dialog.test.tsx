// flux:dialog under the test host: no dialog is shown, the test answers it.
// An answer queued ahead is taken by the next call, a call made first waits
// for the answer, the app gets flux:fs handles named as a picker would name
// them, a cancel is an empty list or null, a second dialog while one is
// open rejects, and the capability tells the truth about a headless client.

import { test, expect } from "@solidrt/test"
import { openDirectory, openFiles, saveFile } from "flux:dialog"
import { dir, file } from "flux:fs"

const TEXT_TYPE = { description: "Text", extensions: ["txt"] }

test("an answer queued ahead is what the next openFiles resolves to, named and readable", async app => {
  await file("notes.txt").write("hello")
  let asked = app.dialog({ files: ["notes.txt"] })
  let picked = await openFiles({ types: [TEXT_TYPE] })
  expect(picked.map(f => f.name)).toEqual(["notes.txt"])
  expect(picked[0]!.path).toBe("notes.txt")
  expect(await picked[0]!.text()).toBe("hello")
  expect(await asked).toEqual({ kind: "openFiles", multiple: false, types: [TEXT_TYPE], startIn: null })
})

test("a call made first waits for the answer; a cancel is an empty list", async app => {
  let picking = openFiles({ multiple: true })
  let asked = app.dialog({ cancel: true })
  expect(await picking).toEqual([])
  expect(await asked).toMatchObject({ kind: "openFiles", multiple: true })
})

test("a { path, name } answer names the handle as a picker would", async app => {
  await file("x.bin").write(new Uint8Array([1, 2, 3]))
  app.dialog({ files: [{ path: "x.bin", name: "Photo.bin" }] })
  let [picked] = await openFiles()
  expect(picked!.name).toBe("Photo.bin")
  expect(picked!.path).toBe("x.bin")
  expect(await picked!.bytes()).toEqual(new Uint8Array([1, 2, 3]))
})

test("saveFile resolves to a handle the app writes through, or null on a cancel", async app => {
  await dir("out").create()
  let asked = app.dialog({ files: ["out/report.csv"] })
  let target = await saveFile({ suggestedName: "report.csv", types: [{ extensions: ["csv"] }] })
  expect(target?.name).toBe("report.csv")
  await target!.write("a,b\n1,2\n", { atomic: true })
  expect(await file("out/report.csv").text()).toBe("a,b\n1,2\n")
  expect(await asked).toEqual({
    kind: "saveFile",
    suggestedName: "report.csv",
    types: [{ description: "csv", extensions: ["csv"] }],
    startIn: null,
  })

  app.dialog({ cancel: true })
  expect(await saveFile()).toBe(null)
})

test("openDirectory resolves to a folder whose entries carry the path that opens them", async app => {
  await dir("docs").create()
  await file("docs/a.txt").write("A")
  let asked = app.dialog({ directory: "docs" })
  let folder = await openDirectory({ startIn: "." })
  expect(folder?.name).toBe("docs")
  let entries = await folder!.entries()
  expect(entries).toEqual([{ name: "a.txt", type: "file", path: "docs/a.txt" }])
  expect(await file(entries[0]!.path).text()).toBe("A")
  expect(await asked).toEqual({ kind: "openDirectory", startIn: "." })

  app.dialog({ cancel: true })
  expect(await openDirectory()).toBe(null)
})

test("one dialog at a time: a second call while one waits rejects as InvalidStateError", async app => {
  let first = openFiles()
  await expect(openFiles()).rejects.toMatchObject({ name: "InvalidStateError" })
  app.dialog({ cancel: true })
  expect(await first).toEqual([])
})

test("the options are validated up front", async () => {
  expect(() => openFiles({ types: [{ extensions: [] }] })).toThrow(/extensions/)
  expect(() => openFiles({ types: [{ extensions: ["*"] }] })).toThrow(/extensions/)
  expect(() => saveFile({ suggestedName: 42 as unknown as string })).toThrow(/suggestedName/)
  // A NUL could not reach the platform's dialog, and SDL would refuse the
  // request after taking the responder: refused up front instead.
  expect(() => saveFile({ suggestedName: "re\0port.csv" })).toThrow(/NUL/)
  expect(() => openFiles({ startIn: "a\0b" })).toThrow(/NUL/)
  expect(() => openFiles({ types: [{ description: "x\0y", extensions: ["txt"] }] })).toThrow(/NUL/)
})

test("a handle's name is read lazily and stays what it was", async () => {
  let f = file("later/report.csv")
  expect(f.name).toBe("report.csv")
  expect(Object.keys(f)).toContain("name")
})

test("a headless test client lists the dialog capability, and a test's dialogs answer", async app => {
  expect(Flux.capabilities.includes("dialog")).toBe(true)
  app.dialog({ cancel: true })
  expect(await openFiles()).toEqual([])
})
