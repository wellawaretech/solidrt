# Badge

A small rounded pill for counts, labels, and status. `variant` picks the role: `primary` (accent fill, the default), `neutral` (subtle surface), `danger`. A string/number child renders as the themed label, anything else as-is (an icon, a dot, ...). Override the fill via `style.backgroundColor` and the label color via `style.color`.

```jsx
import { Badge } from "@solidrt/components"

<Badge>New</Badge>
<Badge variant="danger">Error</Badge>
```
