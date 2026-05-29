declare global {
  type FluxDirEntry = {
    name: string
    type: "file" | "directory" | "symlink" | "other"
  }

  type FluxDir = {
    path: string
    entries(): Promise<FluxDirEntry[]>
    exists(): Promise<boolean>
  }

  type FluxFileStat = {
    size: number
    type: string
    mtime?: number
  }

  type FluxFile = {
    path: string
    text(): Promise<string>
    bytes(): Promise<Uint8Array>
    json(): Promise<any>
    exists(): Promise<boolean>
    stat(): Promise<FluxFileStat>
  }

  type FluxServeOptions = {
    port: number
    fetch?: (req: Request) => Response | string | Promise<Response | string>
  }

  let Flux: {
    on(event: string, callback: (data: any) => void): () => void
    once(event: string, callback: (data: any) => void): () => void
    dir(path: string): FluxDir
    file(path: string): FluxFile
    write(path: string, data: string | Uint8Array): Promise<void>
    serve(options: FluxServeOptions): void
  }
}

export {}