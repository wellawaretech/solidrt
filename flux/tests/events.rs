#![cfg(feature = "compile")]

use flux::{JsEngine, LogLevel};
use std::sync::{Arc, Mutex};

fn capture_log() -> (
    Arc<Mutex<Vec<(LogLevel, String)>>>,
    impl Fn(LogLevel, &str) + Send + Sync + 'static,
) {
    let log = Arc::new(Mutex::new(Vec::<(LogLevel, String)>::new()));
    let log2 = log.clone();
    let f = move |level: LogLevel, msg: &str| {
        log2.lock().unwrap().push((level, msg.to_string()));
    };
    (log, f)
}

fn log_output(log: &[(LogLevel, String)]) -> String {
    log.iter()
        .filter(|(l, _)| *l == LogLevel::Log)
        .map(|(_, m)| m.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

fn run_with_events(
    code: &str,
    channel: &str,
    events: Vec<(&str, u64)>,
) -> String {
    let (log, log_fn) = capture_log();
    let engine = JsEngine::builder()
        .logger(log_fn)
        .event_channel(channel, 1)
        .build();
    let handle = engine.event_handle();

    let code = code.to_string();
    let rt = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap(),
    );
    let rt2 = rt.clone();
    let engine_thread = std::thread::spawn(move || {
        rt2.block_on(engine.eval_source(&code));
    });

    std::thread::sleep(std::time::Duration::from_millis(100));

    for (data, delay_ms) in events {
        if delay_ms > 0 {
            std::thread::sleep(std::time::Duration::from_millis(delay_ms));
        }
        handle.emit(channel, data.to_string());
    }

    engine_thread.join().expect("engine thread panicked");
    let log = log.lock().unwrap();
    log_output(&log)
}

#[test]
fn emit_triggers_listener() {
    let output = run_with_events(
        r#"
        on("test", (data) => {
            console.log("received:" + data.value);
            off("test", 1);
        });
        "#,
        "test",
        vec![(r#"{"value":"hello"}"#, 0)],
    );
    assert_eq!(output, "received:hello");
}

#[test]
fn event_delivery_with_set_interval() {
    let output = run_with_events(
        r#"
        let count = 0;
        let intervalId = setInterval(() => {}, 100);

        let listenerId = on("render", () => {
            count++;
            console.log("render:" + count);
            if (count >= 3) {
                off("render", listenerId);
                clearInterval(intervalId);
            }
        });
        "#,
        "render",
        vec![("{}", 50), ("{}", 50), ("{}", 50)],
    );
    assert!(output.contains("render:3"), "expected 3 render events, got: {output}");
}

#[test]
fn microtask_registered_listener_with_set_interval() {
    let output = run_with_events(
        r#"
        let count = 0;
        let intervalId = setInterval(() => {}, 100);
        let listenerId;

        // Register the event listener inside a microtask, like Solid.js onSettled does
        queueMicrotask(() => {
            listenerId = on("render", () => {
                count++;
                console.log("render:" + count);
                if (count >= 3) {
                    off("render", listenerId);
                    clearInterval(intervalId);
                }
            });
        });
        "#,
        "render",
        vec![("{}", 50), ("{}", 50), ("{}", 50)],
    );
    assert!(output.contains("render:3"), "expected 3 render events with microtask-deferred listener, got: {output}");
}
