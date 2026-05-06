// Stand-alone entry point - compile JS from stdin to bytecode on stdout

fn main() {
    let mut source = String::new();
    std::io::Read::read_to_string(&mut std::io::stdin(), &mut source)
        .unwrap_or_else(|e| {
            eprintln!("error: failed to read stdin: {e}");
            std::process::exit(1);
        });

    let bytecode = flux::compile_source(&source, "stdin");
    std::io::Write::write_all(&mut std::io::stdout(), &bytecode)
        .unwrap_or_else(|e| {
            eprintln!("error: failed to write stdout: {e}");
            std::process::exit(1);
        });
}
