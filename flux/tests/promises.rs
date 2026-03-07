use std::time::Duration;
use qjsrt::run_script;

const TIMEOUT: Option<Duration> = Some(Duration::from_secs(3));

#[test]
fn promise_resolve() {
    let result = run_script(
        r#"
        Promise.resolve('resolved')
        "#,
        TIMEOUT,
    );
    assert_eq!(result, "Promise { 'resolved' }");
}

#[test]
fn promise_then_chain() {
    let result = run_script(
        r#"
        Promise.resolve(1)
            .then(v => v + 1)
            .then(v => v * 3)
        "#,
        TIMEOUT,
    );
    assert_eq!(result, "Promise { 6 }");
}

#[test]
fn promise_catch() {
    let result = run_script(
        r#"
        Promise.reject(new Error('boom'))
            .catch(e => e.message)
        "#,
        TIMEOUT,
    );
    assert_eq!(result, "Promise { 'boom' }");
}

#[test]
fn promise_all() {
    let result = run_script(
        r#"
        Promise.all([
            Promise.resolve('a'),
            Promise.resolve('b'),
            Promise.resolve('c'),
        ])
        "#,
        TIMEOUT,
    );
    assert_eq!(result, "Promise { [ 'a', 'b', 'c' ] }");
}

#[test]
fn async_function() {
    let result = run_script(
        r#"
        (async () => {
            const a = await Promise.resolve('hello');
            const b = await Promise.resolve(' world');
            return a + b;
        })()
        "#,
        TIMEOUT,
    );
    assert_eq!(result, "Promise { 'hello world' }");
}
