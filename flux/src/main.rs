use qjsrt::{run, run_script};

fn main() {
    let mut args = std::env::args().skip(1);
    let code = match args.next().as_deref() {
        Some("-e") => {
            let expr = args.next().unwrap_or_else(|| {
                eprintln!("error: -e requires a JavaScript expression");
                std::process::exit(1);
            });
            run_script(&expr, None);
            return;
        }
        Some("-p") => {
            let expr = args.next().unwrap_or_else(|| {
                eprintln!("error: -p requires a JavaScript expression");
                std::process::exit(1);
            });
            let result = run_script(&expr, None);
            if !result.is_empty() {
                println!("{result}");
            }
            return;
        }
        Some(path) if !path.starts_with('-') => {
            std::fs::read_to_string(path).unwrap_or_else(|e| {
                eprintln!("error: cannot read '{path}': {e}");
                std::process::exit(1);
            })
        }
        Some(flag) => {
            eprintln!("error: unknown flag '{flag}'");
            eprintln!("usage: qjsrt [-e|-p '<javascript>' | <file.js>]");
            std::process::exit(1);
        }
        None => {
            eprintln!("usage: qjsrt [-e|-p '<javascript>' | <file.js>]");
            std::process::exit(1);
        }
    };

    run(&code);
}
