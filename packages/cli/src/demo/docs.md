# srt demo

{{ usage demo }}

The demos that come with the packages this project installs. Without an
argument it lists them, numbered:

```
  1  3d/the-third-dimension
```

`srt demo 1` runs that one, exactly as [srt run](../server/docs.md) runs any
project: a dev server, a local client window, reload on save, and the same
server options (`--lan` to reach it from a device, `--port`, `--tunnel`). The qualified
name works too (`srt demo 3d/the-third-dimension`), which is the stable way to
name one in a script - the numbers follow the list, and the list follows what
is installed.

A package keeps its demos in `demos/`, and that folder is one project: its own
package.json, one `assets/` folder, and `src/<name>.tsx` per demo. So the
demos of one package share a dev server and a port - start a second one while
the first is up and it says so. Demos from different packages run side by
side.

The demos ship inside the packages, so `node_modules` is where they are found:
`srt demo` looks in the current directory and nowhere above it. Run it from
the project root.
