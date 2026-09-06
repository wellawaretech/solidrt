// The label/value list primitives shared by the app detail view and the
// settings screen: a titled card holding right-aligned rows.
import { View, Text, Card, space } from "@solidrt/components"

export function DetailRow(props: { label: string; value: string; mutedValue?: boolean }) {
  return (
    <View layout={{ flexDirection: "row", justifyContent: "space-between", gap: space("md") }}>
      <Text variant="body" muted>
        {props.label}
      </Text>
      <Text variant="body" muted={props.mutedValue}>
        {props.value}
      </Text>
    </View>
  )
}

export function DetailCard(props: { title: string; children?: any }) {
  return (
    <Card layout={{ gap: space("md"), padding: space("lg") }}>
      <Text variant="title" muted>
        {props.title}
      </Text>
      {props.children}
    </Card>
  )
}
