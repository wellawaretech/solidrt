# Coming from HTML

HTML starts from documents: headings, paragraphs, lists and links. SolidRT
starts from the things that a GPU draws. The elements combine parts of HTML,
SVG and canvas.
[HTML: New Beginnings](https://wellawaretech.com/blog/html-new-beginnings/)
gives the reasons for this design.

## The elements

There are two types of elements:

- **Layout elements.** These are as the elements in HTML. The layout engine
  gives each one a position and a size in its parent.
- **Detached elements** (`d-*`). These have no layout box, and the layout
  engine does not see them. You give each one `x`, `y`, `w` and `h` in the
  coordinates of its parent. Without `w` and `h`, a detached element fills
  the box of its parent.

Most elements have the two forms, for example `rect` and `d-rect`. A `span`
is neither. It is a styled run in a `text`.

| Element | Use | Nearest&nbsp;HTML |
| --- | --- | --- |
| `window` | The root of the tree. One for each app | `body` |
| `view`, `d-view` | Groups, clips and transforms other elements | `div` |
| `text`, `d-text` | A paragraph | `p` |
| `span` | A styled run in a `text` | `span` |
| `rect`, `d-rect`<br>`oval`, `d-oval`<br>`line`, `d-line`<br>`path`, `d-path` | Shapes | SVG shapes |
| `texture`, `d-texture` | An image, or pixels that you write | `img`, `canvas` |

Use the layout form for the structure of a screen. Use the detached form for
backgrounds, for content that moves in each frame, and for shapes that you
draw at coordinates, for example a chart.

All shapes use the same paint props: `color` (a color or a gradient),
`drawStyle` (fill, stroke or both), the stroke props and `blendMode`.

The layout is flexbox and a subset of CSS grid. The prop names are the CSS
names, for example `flexDirection`, `gap` and `padding`.

[Core](/core/) describes each element.

## Common errors

**Put text in a `text` element.** A string is the content of a `text` or a
`span`. It is not content of a `view`. A `span` must be inside a `text`:

```tsx
<text>
  Weather for <span fontWeight={700}>Tuesday</span>
</text>
```

**A `view` does not paint.** There is no `backgroundColor`. Put a `d-rect`
in the view, before the content:

```tsx
<view padding={16}>
  <d-rect color="#1b2440" radius={12} />
  <text color="white">Boxed</text>
</view>
```

The `d-rect` fills the view, and it does not move the `text`. Do not use a
`rect` for this. A `rect` is a layout element, and it takes a position in
the column, as a `div` does.

**There is no border.** Draw the border with a second `d-rect`. One shape
has one `color`, so the fill and the stroke cannot have different colors.
Put the stroke after the content, so that the content does not cover it:

```tsx
<view padding={16}>
  <d-rect color="#1b2440" radius={12} />
  <text color="white">Boxed</text>
  <d-rect color="#7ea9ea" drawStyle="stroke" strokeWidth={2} radius={12} />
</view>
```

**A `radius` does not clip the children.** The `radius` rounds only its own
shape. To clip the content of a view, set `overflow="hidden"` and
`clipRadius` on the view. For an image, set `radius` on the `texture`. This
is faster than a clip.

**There is no `img`. Use a `texture`.** The `src` of a `texture` is a
texture id, not a URL. Load the image with `createImage` first:

```tsx
let logo = createImage("https://example.com/logo.png")

<texture src={logo()} />
```

## Components

The elements above are the core of SolidRT. The
[Components](/extensions/components/) extension adds components that are
nearer to HTML and CSS. For example, a `View` takes `backgroundColor`,
`borderColor`, `borderWidth` and `borderRadius` in its `style`. An `Image`
takes a URL in its `src`. The extension also has `Button`, `TextInput`,
`ScrollView`, `Modal` and more.

```tsx
import { View, Image } from "@solidrt/components"

<View
  style={{
    backgroundColor: "#1b2440",
    borderColor: "#7ea9ea",
    borderWidth: 2,
    borderRadius: 12,
  }}
>
  <Image src="https://example.com/logo.png" />
</View>
```
