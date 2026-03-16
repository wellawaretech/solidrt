use std::process::Command;

fn qjsrt_eval(expr: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_qjsrt"))
        .args(["-e", expr])
        .output()
        .expect("failed to run qjsrt")
}

#[test]
fn console_log_prints_to_stdout() {
    let output = qjsrt_eval("console.log('hello')");
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "hello");
    assert!(output.stderr.is_empty());
}

#[test]
fn console_warn_prints_to_stderr() {
    let output = qjsrt_eval("console.warn('warning')");
    assert!(output.stdout.is_empty());
    assert_eq!(String::from_utf8_lossy(&output.stderr).trim(), "warning");
}

#[test]
fn console_error_prints_to_stderr() {
    let output = qjsrt_eval("console.error('oops')");
    assert!(output.stdout.is_empty());
    assert_eq!(String::from_utf8_lossy(&output.stderr).trim(), "oops");
}

#[test]
fn console_log_multiple_args() {
    let output = qjsrt_eval("console.log('a', 'b', 'c')");
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "a b c");
}

#[test]
fn console_log_mixed_types() {
    let output = qjsrt_eval("console.log('count:', 42, true, null)");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "count: 42 true null"
    );
}
