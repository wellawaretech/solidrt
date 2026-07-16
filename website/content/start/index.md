# Start

> Stub: this becomes the five-minute walkthrough. Steps below are the
> intended shape; commands get verified against the shipped CLI.

Build and run your first SolidRT app.

## 1. Scaffold a project

```sh
srt init my-app
cd my-app
bun install
```

Choose the `minimal` template: a window, a line of text, nothing else.

## 2. Run it

```sh
srt
```

A native window opens, connected to the dev server. Leave it running.

## 3. Change something

Open `src/index.tsx` and edit the text:

```tsx
<window>
  <text>Hello from my app!</text>
</window>
```

Save, and the running window updates.

That is the loop: edit, save, see it. Everything else builds on it.

## Where next

- Understand the model: [Core concepts](/core/) - the rendertree, reactive
  values, and why there is no virtual DOM.
- Build faster: [Frameworks](/frameworks/) - ready-made components and
  theming built on top of Core.
