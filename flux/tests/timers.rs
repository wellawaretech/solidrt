use std::time::Duration;
use qjsrt::run_script;

const TIMEOUT: Option<Duration> = Some(Duration::from_secs(3));

#[test]
fn clear_timeout_on_unknown_id_throws() {
    let result = run_script("clearTimeout(999)", TIMEOUT);
    assert!(
        result.starts_with("error:"),
        "expected error for unknown id, got: {result}"
    );
}

#[test]
fn clear_timeout_on_not_yet_fired_cancels() {
    let result = run_script(
        r#"
        const id = setTimeout(() => {}, 100000);
        clearTimeout(id);
        'cancelled'
        "#,
        TIMEOUT,
    );
    assert_eq!(result, "'cancelled'");
}

#[test]
fn clear_timeout_on_unknown_id_caught() {
    let result = run_script(
        r#"
        try { clearTimeout(999); 'no error' } catch (e) { 'caught: ' + e.message }
        "#,
        TIMEOUT,
    );
    assert!(
        result.starts_with("'caught:"),
        "expected caught error, got: {result}"
    );
}

#[test]
fn set_timeout_returns_numeric_id() {
    let result = run_script("typeof setTimeout(() => {}, 1)", TIMEOUT);
    assert_eq!(result, "'number'");
}

#[test]
fn set_interval_returns_numeric_id() {
    let result = run_script("const id = setInterval(() => {}, 1); clearInterval(id); typeof id", TIMEOUT);
    assert_eq!(result, "'number'");
}
