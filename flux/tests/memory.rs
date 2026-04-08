use std::process::Command;

fn qjsrt_module(code: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_qjsrt"))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child.stdin.take().unwrap().write_all(code.as_bytes())?;
            child.wait_with_output()
        })
        .expect("failed to run qjsrt")
}

#[test]
fn import_alloc() {
    let output = qjsrt_module(
        r#"
        import { alloc } from "qjs:memory";
        let buf = alloc(16);
        console.log(buf.byteLength);
        "#,
    );
    assert!(output.stderr.is_empty(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "16");
}

#[test]
fn import_memset() {
    let output = qjsrt_module(
        r#"
        import { alloc, memset } from "qjs:memory";
        let buf = alloc(4);
        memset(buf, 0, 4, 0xAB);
        console.log(buf[0], buf[1], buf[2], buf[3]);
        "#,
    );
    assert!(output.stderr.is_empty(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "171 171 171 171");
}

#[test]
fn import_memset32() {
    let output = qjsrt_module(
        r#"
        import { alloc, memset32 } from "qjs:memory";
        let buf = alloc(8);
        memset32(buf, 0, 2, 0x01020304);
        console.log(buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7]);
        "#,
    );
    assert!(output.stderr.is_empty(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    // 0x01020304 in little-endian bytes: 4, 3, 2, 1
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "4 3 2 1 4 3 2 1"
    );
}

#[test]
fn memset_offset() {
    let output = qjsrt_module(
        r#"
        import { alloc, memset } from "qjs:memory";
        let buf = alloc(8);
        memset(buf, 2, 3, 0xFF);
        console.log(buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7]);
        "#,
    );
    assert!(output.stderr.is_empty(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "0 0 255 255 255 0 0 0"
    );
}

#[test]
fn import_free() {
    let output = qjsrt_module(
        r#"
        import { alloc, free, memset } from "qjs:memory";
        let buf = alloc(4);
        memset(buf, 0, 4, 0x11);
        console.log(buf.byteLength);
        free(buf);
        console.log(buf.byteLength);
        "#,
    );
    assert!(output.stderr.is_empty(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "4\n0"
    );
}

#[test]
fn memset_out_of_bounds() {
    let output = qjsrt_module(
        r#"
        import { alloc, memset } from "qjs:memory";
        let buf = alloc(4);
        try {
            memset(buf, 2, 4, 0xFF);
            console.log("no error");
        } catch (e) {
            console.log(String(e));
        }
        "#,
    );
    assert!(output.stderr.is_empty(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "memset: offset + length out of bounds"
    );
}
