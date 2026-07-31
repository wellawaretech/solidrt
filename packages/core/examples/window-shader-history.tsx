// `previous: true` on the window shader retains the last resolved frame as a
// second layer the program samples as uPrevious, rotated each frame - a
// one-frame history. Here it draws a motion echo behind the orbiting square;
// click to toggle the echo term off and compare with the plain frame.
import { render, onFrame, createSignal } from "@solidrt/core"
import { compileShader, destroyShader, glsl, linkProgram } from "@solidrt/core/gpu"

let VERTEX = glsl`#version 300 es
  precision highp float;
  out vec2 vUV;
  void main() {
    vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
    // uSource/uPrevious are top-left origin; flip v so the frame lands upright.
    vUV = vec2(p.x, 1.0 - p.y);
    gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
  }
`

let ECHO = glsl`
  uniform sampler2D uSource;
  uniform sampler2D uPrevious;
  uniform float uEcho;
  in vec2 vUV;
  void main() {
    vec4 cur = texture(uSource, vUV);
    vec4 prev = texture(uPrevious, vUV);
    // Brightest of the current frame and the decayed previous one: motion
    // leaves a one-frame ghost trailing it.
    fragColor = max(cur, prev * uEcho);
  }
`

function App() {
  let vs = compileShader("vertex", VERTEX)
  let fs = compileShader("fragment", ECHO, { header: true })
  let echoProgram = linkProgram(vs, fs, { label: "echo" })
  destroyShader(vs)
  destroyShader(fs)

  let [angle, setAngle] = createSignal(0)
  let [echo, setEcho] = createSignal(0.65)
  onFrame(tick => setAngle(tick / 350))

  return (
    <window
      shader={{ program: echoProgram, params: { uEcho: echo() }, previous: true }}
      onPointerDown={() => setEcho(e => (e > 0 ? 0 : 0.65))}
      alignItems="center"
      justifyContent="center"
    >
      <rect position="absolute" top={0} right={0} bottom={0} left={0} color="#101826" />
      <view width={70} height={70} x={Math.cos(angle()) * 150} y={Math.sin(angle()) * 150}>
        <rect width={70} height={70} radius={16} color="#7ad0ff" />
      </view>
      <text position="absolute" bottom={24} fontSize={14} color="#99aabb">
        Click to toggle the uPrevious echo
      </text>
    </window>
  )
}

render(() => <App />)
