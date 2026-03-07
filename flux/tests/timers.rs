use std::time::Duration;
use qjsrt::{run, RunOptions};

const TEST_TIMEOUT: RunOptions = RunOptions {
    timeout: Some(Duration::from_secs(3)),
};

#[test]
fn clear_timeout_on_unknown_id_throws() {
    let result = run("clearTimeout(999)", Some(TEST_TIMEOUT));
    assert!(
        result.starts_with("error:"),
        "expected error for unknown id, got: {result}"
    );
}

#[test]
fn clear_timeout_on_not_yet_fired_cancels() {
    let result = run(
        r#"
        const id = setTimeout(() => {}, 100000);
        clearTimeout(id);
        'cancelled'
        "#,
        Some(TEST_TIMEOUT),
    );
    assert_eq!(result, "cancelled");
}

#[test]
fn clear_timeout_on_unknown_id_caught() {
    let result = run(
        r#"
        try { clearTimeout(999); 'no error' } catch (e) { 'caught: ' + e.message }
        "#,
        Some(TEST_TIMEOUT),
    );
    assert!(
        result.starts_with("caught:"),
        "expected caught error, got: {result}"
    );
}

#[test]
fn set_timeout_returns_numeric_id() {
    let result = run("typeof setTimeout(() => {}, 1)", Some(TEST_TIMEOUT));
    assert_eq!(result, "number");
}

#[test]
fn set_interval_returns_numeric_id() {
    let result = run("typeof setInterval(() => {}, 1)", Some(TEST_TIMEOUT));
    assert_eq!(result, "number");
}
