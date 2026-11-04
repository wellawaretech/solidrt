// flux:link's openLink and the navigator locale. The test host records
// links instead of opening them, so a call resolves without a browser
// showing up, and the capability is listed whatever the machine has.
import { expect, test } from "flux:test"
import { openLink } from "flux:link"

test("the test host lists the link capability", () => {
  expect(Flux.capabilities.includes("link")).toBe(true)
})

test("openLink resolves for a link with a scheme", async () => {
  expect(await openLink("https://example.com/credits")).toBeUndefined()
  expect(await openLink("mailto:feedback@example.com")).toBeUndefined()
})

test("openLink throws on a link without a scheme", () => {
  expect(() => openLink("/settings")).toThrow(TypeError)
  expect(() => openLink("C:\\Windows\\notepad.exe")).toThrow(TypeError)
  expect(() => openLink("example.com")).toThrow("openLink: the link needs a scheme, like https://example.com, got \"example.com\"")
})

test("openLink throws on a link that is not a string", () => {
  expect(() => openLink(42 as unknown as string)).toThrow("openLink: pass the link as a string")
})

test("navigator.languages lists BCP 47 tags, navigator.language is the first", () => {
  let languages = navigator.languages
  expect(languages.length > 0).toBe(true)
  for (let tag of languages) expect(/^[A-Za-z]+(-[A-Za-z0-9]+)*$/.test(tag)).toBe(true)
  expect(navigator.language).toBe(languages[0])
})

test("navigator.languages is frozen", () => {
  expect(Object.isFrozen(navigator.languages)).toBe(true)
})
