# Coming from Expo/React Native

The element model is almost the same as the React Native model: a small
set of native elements, with flexbox layout.

The role of JavaScript is different. SolidRT is an engine written in Rust,
and JavaScript is its scripting language. In React Native, the app is
JavaScript, and the native side serves it. In SolidRT, the engine does the
work of each frame, for example the animation and the layout, and your
JavaScript coordinates it. [Performance](/guides/performance/) explains
this model.

If you build with Expo, the workflow is also familiar. There is a dev
server, a client app that connects to the server, and one command that
builds an installable app. The client runs on all devices: Windows, macOS
and Linux computers, and Android phones, tablets and TVs. You can also run
many clients on one computer, each in a different window size. For example,
`bun run client --size 390x844` opens a window with the size of a phone.

SolidRT apps use SolidJS, not React. For that part, see
[Coming from React](/guides/coming-from-react/).

## Elements and layout

There are two layers:

- The [Components](/extensions/components/) extension is nearest to React
  Native. It has `View`, `Text`, `Image`, `Pressable` and more. A component
  takes two objects: `layout` for the layout values, and `style` for the
  paint values, for example `backgroundColor`, `borderColor`, `borderWidth`
  and `borderRadius`.
- Core has the elements of the components: `view`, `text`, `rect`,
  `texture` and the others. A `view` does not paint. Some elements have a
  layout box, and some are detached.
  [Coming from HTML](/guides/coming-from-html/) gives an overview, and the
  usual errors.

| React Native | Components | Core |
| --- | --- | --- |
| `<View style>` | `<View layout style>` | `<view>` with a `<d-rect>` in it |
| `<Text>` | `<Text>` | `<text>`, with `<span>` in it |
| `<Image>`, `expo-image` | `<Image src>` | `<texture>` with `createImage` |
| `<Pressable onPress>` | `<Pressable onPress>` | `onPointerDown`, `onPointerUp` |
| `<ScrollView>` | `<ScrollView>` | `createScroll` |
| `<TextInput>` | `<TextInput>` | `onTextInput` |
| `<Modal>` | `<Modal>` | |
| `SafeAreaView` | `<SafeArea>` | `safeArea()` |
| `<FlatList>` | `<For>` in a `<ScrollView>`, not virtualized | |

The layout is flexbox, and `flexDirection` is `column` by default, as in
React Native. SolidRT also has a subset of CSS grid.

## Animation, input and the window

| React Native | SolidRT |
| --- | --- |
| `Animated`, Reanimated | The `transition` prop: springs and tweens that the runtime runs, also for enter and exit. For motion in each frame, `onFrame` moves the [detached elements](/core/detached/) (`d-rect`, `d-view`, ...) without a layout pass |
| react-native-gesture-handler | `createPan`, `createSwipe`, `createLongPress`, `createDoubleTap` and `createTransform` in Core |
| `useWindowDimensions` | `windowSize()` |
| `PixelRatio` | `displayScale()` |
| `Keyboard`, `KeyboardAvoidingView` | `keyboardHeight()`: the height of the on-screen keyboard over the window |
| `BackHandler` | `onBack` |
| `Linking`, for links into the app | `onLink`, and `env.launchLink` for the link that started the app |
| React Navigation, Expo Router | `@solidrt/router` |

## Device APIs

| Expo | SolidRT |
| --- | --- |
| `expo-camera` | `createCamera` from `@solidrt/core/camera` |
| Barcode scan in `expo-camera` | The `scan: ["qr"]` option of `createCamera`. It reads only QR codes |
| `expo-audio`, `expo-av` | `createSound` from `@solidrt/core/audio`, `createVideo` from `@solidrt/core/video` |
| Recording in `expo-audio` | `createMicrophone` from `@solidrt/core/microphone` |
| `expo-file-system` | `flux:fs` |
| `expo-document-picker` | `flux:dialog` |
| `expo-sqlite` | `flux:sqlite` |
| AsyncStorage | There is no `localStorage`. Use `flux:sqlite` or `flux:fs` |
| `expo-clipboard` | `navigator.clipboard`, for text only |
| `fetch` | `fetch`, the same standard API |

[Runtime](/runtime/) gives the documentation for the `flux:*` modules.

## Workflow

| Expo | SolidRT |
| --- | --- |
| `npx create-expo-app` | `bun create solidrt` |
| `npx expo start` | `bun run dev` |
| Expo Go | The SolidRT player, which connects to your dev server |
| Fast Refresh | When you save, the app loads again. The state starts again from the initial values (see [Dev loop](/guides/dev-loop/)) |
| `app.json` | The `solidrt` key in `package.json`: `appId`, `icon`, `capabilities`, `android.versionCode` |
| EAS Build | `bun run pack --apk`, on your computer, without the Android SDK |

The command signs a packed APK with a development key. This key is
satisfactory to sideload the APK. At this time, you cannot publish to
Google Play. Google Play accepts only an Android App Bundle that you sign
with your own key. `sol pack` does not make an App Bundle yet. See
[sol pack](/tools/pack/).

## What SolidRT adds

- A subset of CSS grid. React Native has only flexbox.
- [Shaders](/core/shaders/) on elements and on the window. React Native
  needs a third-party library for shaders.
- Threads for JavaScript. `flux:isolate` runs a module on its own thread.
- WebAssembly modules, through `flux:wasm`.
- Desktop apps from the same code. Expo does not build desktop apps.
- The [2D](/extensions/2d/) and [3D](/extensions/3d/) extensions.

## What SolidRT does not have

- No iOS build.
- No over-the-air updates, as with `expo-updates`. A new version is a new
  APK.
- No publication to Google Play yet. See [Workflow](#workflow).
- No virtualized list, as with `FlatList`. `<For>` makes a node for each
  item.
- No accessibility support. There are no accessibility props, and screen
  readers cannot read the app.
