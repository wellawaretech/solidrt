# Card

A themed surface container: a padded column box with a `surface` fill, a subtle `border` stroke, and rounded corners, recoloring live on a theme switch. Pass a `title` for a heading, or lay out the content yourself; override paint via `style`, spacing/sizing via `layout`.

```jsx
import { Card } from "@solidrt/components"

<Card title="Profile" layout={{ width: 360 }}>
  <Text>Card body content.</Text>
</Card>
```
