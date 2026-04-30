// Tests commented out — they depend on run_script (script eval mode).

// use std::time::Duration;
// use qjsrt::run_script;
//
// const TIMEOUT: Option<Duration> = Some(Duration::from_secs(3));
//
// #[test]
// fn clear_timeout_on_unknown_id_throws() {
//     let result = run_script("clearTimeout(999)", TIMEOUT);
//     assert!(
//         result.starts_with("error:"),
//         "expected error for unknown id, got: {result}"
//     );
// }
//
// #[test]
// fn clear_timeout_on_not_yet_fired_cancels() {
//     let result = run_script(
//         r#"
//         let id = setTimeout(() => {}, 100000);
//         clearTimeout(id);
//         'cancelled'
//         "#,
//         TIMEOUT,
//     );
//     assert_eq!(result, "'cancelled'");
// }
//
// #[test]
// fn clear_timeout_on_unknown_id_caught() {
//     let result = run_script(
//         r#"
//         try { clearTimeout(999); 'no error' } catch (e) { 'caught: ' + e.message }
//         "#,
//         TIMEOUT,
//     );
//     assert!(
//         result.starts_with("'caught:"),
//         "expected caught error, got: {result}"
//     );
// }
//
// #[test]
// fn set_timeout_returns_numeric_id() {
//     let result = run_script("typeof setTimeout(() => {}, 1)", TIMEOUT);
//     assert_eq!(result, "'number'");
// }
//
// #[test]
// fn set_interval_returns_numeric_id() {
//     let result = run_script("let id = setInterval(() => {}, 1); clearInterval(id); typeof id", TIMEOUT);
//     assert_eq!(result, "'number'");
// }
//
// #[test]
// fn queue_microtask_runs_before_timers() {
//     let result = run_script(
//         r#"
//         let order = [];
//         setTimeout(() => order.push('timeout'), 0);
//         queueMicrotask(() => order.push('microtask'));
//         new Promise(resolve => {
//             setTimeout(() => resolve(order.join(',')), 50);
//         })
//         "#,
//         TIMEOUT,
//     );
//     assert_eq!(result, "Promise { 'microtask,timeout' }");
// }

use std::io::Write;
use std::process::Command;
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(5);

fn qjsrt_module_timeout(code: &str) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_qjsrt"))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to spawn qjsrt");

    child.stdin.take().unwrap().write_all(code.as_bytes()).unwrap();

    let start = std::time::Instant::now();
    loop {
        match child.try_wait().expect("failed to poll child") {
            Some(status) => {
                let mut stdout = Vec::new();
                let mut stderr = Vec::new();
                std::io::Read::read_to_end(&mut child.stdout.take().unwrap(), &mut stdout).unwrap();
                std::io::Read::read_to_end(&mut child.stderr.take().unwrap(), &mut stderr).unwrap();
                return std::process::Output { status, stdout, stderr };
            }
            None if start.elapsed() > TIMEOUT => {
                child.kill().expect("failed to kill child");
                panic!("qjsrt timed out after {TIMEOUT:?} — engine likely froze");
            }
            None => std::thread::sleep(Duration::from_millis(50)),
        }
    }
}

#[test]
fn set_timeout_fires() {
    let output = qjsrt_module_timeout(
        "setTimeout(() => console.log('fired'), 10);",
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "fired");
}

#[test]
fn set_timeout_chained() {
    let output = qjsrt_module_timeout(r#"
        setTimeout(() => {
            setTimeout(() => console.log("chained"), 10);
        }, 10);
    "#);
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "chained");
}

#[test]
fn promise_with_timer() {
    let output = qjsrt_module_timeout(r#"
        let p = new Promise(resolve => setTimeout(() => resolve("ok"), 10));
        p.then(v => console.log(v));
    "#);
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "ok");
}

#[test]
fn multiple_concurrent_timers() {
    let output = qjsrt_module_timeout(r#"
        let results = [];
        setTimeout(() => results.push("a"), 10);
        setTimeout(() => results.push("b"), 20);
        setTimeout(() => {
            results.push("c");
            console.log(results.join(","));
        }, 30);
    "#);
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "a,b,c");
}

#[test]
fn microtask_after_timer() {
    let output = qjsrt_module_timeout(r#"
        setTimeout(() => {
            Promise.resolve().then(() => console.log("microtask"));
        }, 10);
    "#);
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "microtask");
}

#[test]
fn deep_promise_chain_after_timer() {
    let output = qjsrt_module_timeout(r#"
        setTimeout(() => {
            Promise.resolve("a")
                .then(v => v + ",b")
                .then(v => v + ",c")
                .then(v => v + ",d")
                .then(v => console.log(v));
        }, 10);
    "#);
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "a,b,c,d");
}

#[test]
fn queue_microtask_after_timer() {
    let output = qjsrt_module_timeout(r#"
        setTimeout(() => {
            queueMicrotask(() => {
                queueMicrotask(() => {
                    console.log("nested microtask");
                });
            });
        }, 10);
    "#);
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "nested microtask");
}

#[test]
fn microtask_triggers_state_update() {
    let output = qjsrt_module_timeout(r#"
        let state = "initial";
        setTimeout(() => {
            Promise.resolve().then(() => { state = "updated"; });
            setTimeout(() => console.log(state), 50);
        }, 10);
    "#);
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "updated");
}

#[test]
fn async_await_after_timer() {
    let output = qjsrt_module_timeout(r#"
        async function work() {
            let result = await new Promise(resolve =>
                setTimeout(() => resolve("step1"), 10)
            );
            result = await Promise.resolve(result + ",step2");
            result = await Promise.resolve(result + ",step3");
            console.log(result);
        }
        work();
    "#);
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "step1,step2,step3");
}
