// Typecheck-only: locks the JSX types of the `detached` attribute. Each paired
// tag is a union discriminated on the literal `detached`: the laid-out form
// takes layout props and rejects the paint-space geometry, the detached form
// takes the geometry and rejects layout props, and a tag with no detached form
// rejects the attribute. No runner executes this file (it is not a
// `.test.tsx`); the package typecheck does, and a `@ts-expect-error` line that
// stops erroring fails it.

export function detachedAttributeTypes() {
  let draw: { d: string } = { d: "M0 0 L1 1" }
  return [
    <rect color="red" width={10} />,
    <rect detached x={3} w={10} color="red" />,
    <rect detached={false} width={10} />,
    <view detached x={1} />,
    <line detached x1={0} y1={0} x2={1} y2={1} />,
    <text detached anchor="middle">a</text>,
    <path detached {...draw} />,
    <d-rect x={3} />,

    // Geometry on the laid-out form.
    // @ts-expect-error
    <rect x={3} color="red" />,
    // @ts-expect-error
    <line x1={0} y1={0} color="red" />,
    // A layout prop on the detached form.
    // @ts-expect-error
    <rect detached flexGrow={1} />,
    // @ts-expect-error
    <view detached flexGrow={1} />,
    // No detached form.
    // @ts-expect-error
    <span detached />,
    // @ts-expect-error
    <window detached />,
    // A d-* tag is the detached form already.
    // @ts-expect-error
    <d-rect detached />
  ]
}
