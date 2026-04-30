use qjsrt::JsEngine;
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn make_runtime() -> Arc<tokio::runtime::Runtime> {
    Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap(),
    )
}

fn capture_log() -> (Arc<Mutex<Vec<String>>>, impl Fn(qjsrt::LogLevel, &str) + Send + Sync + 'static) {
    let log = Arc::new(Mutex::new(Vec::<String>::new()));
    let log2 = log.clone();
    let f = move |_level: qjsrt::LogLevel, msg: &str| {
        log2.lock().unwrap().push(msg.to_string());
    };
    (log, f)
}

#[test]
fn emit_triggers_listener() {
    let rt = make_runtime();
    let (log, log_fn) = capture_log();
    let engine = JsEngine::builder(rt.clone())
        .log(log_fn)
        .event_channel("test", 16)
        .build();

    rt.block_on(async {
        engine.eval_detached(r#"
            on("test", (data) => {
                console.log("received:" + data.value);
                off("test", 1);
            });
        "#);
        tokio::time::sleep(Duration::from_millis(50)).await;

        engine.emit("test", r#"{"value":"hello"}"#.to_string());

        tokio::time::sleep(Duration::from_millis(200)).await;
        engine.shutdown().await;
    });

    let output = log.lock().unwrap();
    assert!(output.contains(&"received:hello".to_string()), "expected listener output, got: {output:?}");
}

#[test]
fn emit_triggers_microtasks() {
    let rt = make_runtime();
    let (log, log_fn) = capture_log();
    let engine = JsEngine::builder(rt.clone())
        .log(log_fn)
        .event_channel("render", 16)
        .build();

    rt.block_on(async {
        engine.eval_detached(r#"
            let state = "initial";
            on("render", (_data) => {
                Promise.resolve().then(() => {
                    state = "updated";
                    console.log("state:" + state);
                    off("render", 1);
                });
            });
        "#);
        tokio::time::sleep(Duration::from_millis(50)).await;

        engine.emit("render", "{}".to_string());

        tokio::time::sleep(Duration::from_millis(500)).await;
        engine.shutdown().await;
    });

    let output = log.lock().unwrap();
    assert!(output.contains(&"state:updated".to_string()), "microtask did not run — got: {output:?}");
}

#[test]
fn emit_chained_microtasks() {
    let rt = make_runtime();
    let (log, log_fn) = capture_log();
    let engine = JsEngine::builder(rt.clone())
        .log(log_fn)
        .event_channel("frame", 16)
        .build();

    rt.block_on(async {
        engine.eval_detached(r#"
            let log = [];
            on("frame", (_data) => {
                Promise.resolve("a")
                    .then(v => { log.push(v); return "b"; })
                    .then(v => { log.push(v); return "c"; })
                    .then(v => {
                        log.push(v);
                        console.log(log.join(","));
                        off("frame", 1);
                    });
            });
        "#);
        tokio::time::sleep(Duration::from_millis(50)).await;

        engine.emit("frame", "{}".to_string());

        tokio::time::sleep(Duration::from_millis(500)).await;
        engine.shutdown().await;
    });

    let output = log.lock().unwrap();
    assert!(output.contains(&"a,b,c".to_string()), "chained microtasks did not complete — got: {output:?}");
}

#[test]
fn emit_multiple_frames() {
    let rt = make_runtime();
    let (log, log_fn) = capture_log();
    let engine = JsEngine::builder(rt.clone())
        .log(log_fn)
        .event_channel("frame", 16)
        .build();

    rt.block_on(async {
        engine.eval_detached(r#"
            let count = 0;
            on("frame", (_data) => {
                count++;
                Promise.resolve().then(() => {
                    console.log("frame:" + count);
                    if (count >= 3) {
                        off("frame", 1);
                    }
                });
            });
        "#);
        tokio::time::sleep(Duration::from_millis(50)).await;

        for _ in 0..3 {
            engine.emit("frame", "{}".to_string());
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        tokio::time::sleep(Duration::from_millis(500)).await;
        engine.shutdown().await;
    });

    let output = log.lock().unwrap();
    assert!(output.contains(&"frame:3".to_string()), "not all frames processed — got: {output:?}");
}
