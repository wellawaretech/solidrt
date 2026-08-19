# Field

A form row: `label` above the control, the control itself (`children`, rendered as-is), and a help or error line below. `error` renders in the danger color and replaces `description` while set. It draws no chrome and does not reach into the control - error styling of the input itself stays the input's `style` prop, no hidden magic. The message line only occupies space while there is one; reserve the space with a constant `description` if the form must not jump when an error appears.

```jsx
import { Field, TextInput } from "@solidrt/components"

<Field label="Email" description="Used for receipts only." error={emailError()}>
  <TextInput value={email()} onInput={setEmail} />
</Field>
```
