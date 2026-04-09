use qjsrt::{run, run_bytecode};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let mut mode = None;
    let mut output = None;
    let mut input = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-c" | "-b" => mode = Some(args[i].clone()),
            // "-e" | "-p" => mode = Some(args[i].clone()),
            "-o" => {
                i += 1;
                output = Some(args.get(i).unwrap_or_else(|| {
                    eprintln!("error: -o requires an output path");
                    std::process::exit(1);
                }).clone());
            }
            arg if !arg.starts_with('-') => input = Some(arg.to_string()),
            flag => {
                eprintln!("error: unknown flag '{flag}'");
                std::process::exit(1);
            }
        }
        i += 1;
    }

    match mode.as_deref() {
        // #[cfg(feature = "script")]
        // Some("-e") => {
        //     let expr = input.unwrap_or_else(|| {
        //         eprintln!("error: -e requires a JavaScript expression");
        //         std::process::exit(1);
        //     });
        //     run_script(&expr, None);
        // }
        // #[cfg(feature = "script")]
        // Some("-p") => {
        //     let expr = input.unwrap_or_else(|| {
        //         eprintln!("error: -p requires a JavaScript expression");
        //         std::process::exit(1);
        //     });
        //     let result = run_script(&expr, None);
        //     if !result.is_empty() {
        //         println!("{result}");
        //     }
        // }
        Some("-b") => {
            let path = input.unwrap_or_else(|| {
                eprintln!("error: -b requires a bytecode file path");
                std::process::exit(1);
            });
            let bytecode = std::fs::read(&path).unwrap_or_else(|e| {
                eprintln!("error: cannot read '{path}': {e}");
                std::process::exit(1);
            });
            run_bytecode(bytecode);
        }
        Some("-c") => {
            let (source, name, default_out) = match &input {
                Some(path) => {
                    let s = std::fs::read_to_string(path).unwrap_or_else(|e| {
                        eprintln!("error: cannot read '{path}': {e}");
                        std::process::exit(1);
                    });
                    (s, path.clone(), Some(path.replace(".js", ".bin")))
                }
                None => {
                    let mut s = String::new();
                    std::io::Read::read_to_string(&mut std::io::stdin(), &mut s)
                        .unwrap_or_else(|e| {
                            eprintln!("error: failed to read stdin: {e}");
                            std::process::exit(1);
                        });
                    let name = output.clone().unwrap_or_else(|| "stdin".into());
                    (s, name, None)
                }
            };
            let out = output.or(default_out).unwrap_or_else(|| {
                eprintln!("error: -o required when compiling from stdin");
                std::process::exit(1);
            });
            let bytecode = qjsrt::compile_source(&source, &name);
            std::fs::write(&out, &bytecode).unwrap_or_else(|e| {
                eprintln!("error: cannot write '{out}': {e}");
                std::process::exit(1);
            });
            println!("wrote {} bytes to {out}", bytecode.len());
        }
        Some(_) => unreachable!(),
        None => {
            let code = match &input {
                Some(path) => std::fs::read_to_string(path).unwrap_or_else(|e| {
                    eprintln!("error: cannot read '{path}': {e}");
                    std::process::exit(1);
                }),
                None => {
                    let mut s = String::new();
                    std::io::Read::read_to_string(&mut std::io::stdin(), &mut s)
                        .unwrap_or_else(|e| {
                            eprintln!("error: failed to read stdin: {e}");
                            std::process::exit(1);
                        });
                    s
                }
            };
            run(&code);
        }
    }
}
