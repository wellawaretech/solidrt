import { Logo } from "@solidrt/core"
import { View, Text } from "@solidrt/components"

export function Home() {
  return (
    <View layout={{ flex: 1, gap: 20, alignItems: "center", justifyContent: "center" }}>
      <Logo size={300} animation="loop" />
      <Text layout={{ fontSize: 40 }} style={{ color: "#ccc" }}>
        The Solid Runtime
      </Text>
    </View>
  )
}
