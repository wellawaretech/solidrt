# @solidrt/components examples

Single-concept SolidRT patterns using @solidrt/components. Each file is a
complete, runnable app (ends in `render(() => <App />)`) demonstrating exactly
one thing - copy one and adapt it. For core primitives see
`@solidrt/core/examples`; for the component prop model see
`@solidrt/components/AGENTS.md`.

## Theme
- `theme-toggle.tsx` - a `Switch` flipping `setTheme` between `darkTheme` and
  `lightTheme`; every component recolors reactively, no remount.
