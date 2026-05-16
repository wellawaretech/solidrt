# @solidrt/cli

Developer tooling for SolidRT. Provides a development environment for `@solidrt/core` applications.

## Commands

```sh
bunx srt run [file]          # start dev server + client, optionally load a file
bunx srt run --server [file] # start dev server only
bunx srt run --client        # start dev client only
```

## REPL

Running `bunx srt run` opens an interactive REPL. The loaded file is watched for changes and automatically pushed to connected clients.

| Command         | Description                                      |
| --------------- | ------------------------------------------------ |
| `load <file>`   | load and push a `.tsx`, `.srt.js`, or `.srt.bin` |
| `reload [n]`    | rebuild and push to all clients, or client `n`   |
| `stop [n]`      | stop all clients, or client `n`                  |
| `list`          | list connected clients                           |
| `!<cmd>`        | run a shell command                              |
| `quit` / `exit` | exit the dev server                              |
