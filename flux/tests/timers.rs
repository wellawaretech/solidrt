use std::time::Duration;
use qjsrt::{run_with_options, RunOptions};

const TEST_TIMEOUT: RunOptions = RunOptions {
    timeout: Some(Duration::from_secs(5)),
};

#[test]
fn clear_timeout_on_unknown_id_throws() {
    let result = run_with_options("clearTimeout(999)", TEST_TIMEOUT);
    assert!(
        result.starts_with("error:"),
        "expected error for unknown id, got: {result}"
    );
}

#[test]
fn clear_timeout_on_not_yet_fired_cancels() {
    let result = run_with_options(
        r#"
        const id = setTimeout(() => {}, 100000);
        clearTimeout(id);
        'cancelled'
        "#,
        TEST_TIMEOUT,
    );
    assert_eq!(result, "cancelled");
}

#[test]
fn set_timeout_returns_numeric_id() {
    let result = run_with_options("typeof setTimeout(() => {}, 1)", TEST_TIMEOUT);
    assert_eq!(result, "number");
}

#[test]
fn set_interval_returns_numeric_id() {
    let result = run_with_options("typeof setInterval(() => {}, 1)", TEST_TIMEOUT);
    assert_eq!(result, "number");
}
