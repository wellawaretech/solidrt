# Carousel

A pager. The children are the pages, each one box wide, laid side by side and moved with the finger: a horizontal swipe (core's swipe recognizer) turns the page, a drag that stops short snaps to the nearest one, and the page settles under a spring. Only horizontal drags are taken, so a vertical ScrollView around or inside it scrolls as before. `index` controls the page (pair it with `onChange`); without it the carousel keeps its own page and still reports turns.

```jsx
import { Carousel, Card, Text } from "@solidrt/components"

<Carousel layout={{ height: 200 }} onChange={(i) => setPage(i)}>
  <Card title="One"><Text>First page</Text></Card>
  <Card title="Two"><Text>Second page</Text></Card>
  <Card title="Three"><Text>Third page</Text></Card>
</Carousel>
```

Page indicators are the app's: read the index from `onChange` and draw dots beside it.
