use rquickjs::{Ctx, JsLifetime};
use std::any::Any;
use std::sync::{Arc, Mutex};

use crate::logger::{default_logger, CtxLogger, LogFn, LogLevel, Logger};
use crate::plugins::events::EventChannels;
use crate::plugins::{self, PluginFn, UserdataFn};

type ShutdownFn = Box<dyn FnOnce(&Logger) + Send>;

#[derive(Clone, JsLifetime)]
pub struct ShutdownHooks {
    #[qjs(skip_trace)]
    inner: Arc<Mutex<Vec<ShutdownFn>>>,
}

impl ShutdownHooks {
    fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn add<F: FnOnce(&Logger) + Send + 'static>(&self, f: F) {
        self.inner.lock().unwrap().push(Box::new(f));
    }

    fn run(self, logger: &Logger) {
        for hook in self.inner.lock().unwrap().drain(..) {
            hook(logger);
        }
    }
}

pub fn on_shutdown<F: FnOnce(&Logger) + Send + 'static>(ctx: &Ctx<'_>, f: F) {
    ctx.userdata::<ShutdownHooks>().unwrap().add(f);
}

pub struct JsEngineBuilder {
    plugins: Vec<PluginFn>,
    userdata: Vec<UserdataFn>,
    event_channels: Vec<(String, usize, bool)>,
    logger: Option<LogFn>,
    stack_size: Option<usize>,
}

impl JsEngineBuilder {
    pub fn plugin<F>(mut self, f: F) -> Self
    where
        F: for<'js> FnOnce(Ctx<'js>) + Send + 'static,
    {
        self.plugins.push(Box::new(f));
        self
    }

    /// Store a value in the JS context's userdata before any plugins run.
    /// Plugins can then retrieve it with `ctx.userdata::<T>()`.
    pub fn userdata<T>(mut self, value: T) -> Self
    where
        T: for<'js> JsLifetime<'js> + Send + 'static,
        for<'js> <T as JsLifetime<'js>>::Changed<'static>: Any,
    {
        self.userdata.push(Box::new(move |ctx| {
            ctx.store_userdata(value)
                .expect("failed to store userdata");
        }));
        self
    }

    pub fn logger<F: Fn(LogLevel, &str) + Send + Sync + 'static>(mut self, f: F) -> Self {
        self.logger = Some(Box::new(f));
        self
    }

    pub fn event_channel(mut self, event: &str, capacity: usize) -> EventChannelConfig {
        self.event_channels
            .push((event.to_string(), capacity, false));
        EventChannelConfig { builder: self }
    }

    pub fn stack_size(mut self, limit: usize) -> Self {
        self.stack_size = Some(limit);
        self
    }

    pub fn build(self) -> JsEngine {
        let logger = match self.logger {
            Some(f) => Logger(Arc::from(f)),
            None => default_logger(),
        };
        let event_channels = Arc::new(EventChannels::new(self.event_channels));

        JsEngine {
            setups: self.plugins,
            userdata: self.userdata,
            event_channels,
            logger,
            stack_size: self.stack_size,
        }
    }
}

pub struct EventChannelConfig {
    builder: JsEngineBuilder,
}

impl EventChannelConfig {
    pub fn trace(mut self) -> JsEngineBuilder {
        self.builder.event_channels.last_mut().unwrap().2 = true;
        self.builder
    }

    pub fn plugin<F>(self, f: F) -> JsEngineBuilder
    where
        F: for<'js> FnOnce(Ctx<'js>) + Send + 'static,
    {
        self.builder.plugin(f)
    }

    pub fn event_channel(self, event: &str, capacity: usize) -> EventChannelConfig {
        self.builder.event_channel(event, capacity)
    }

    pub fn build(self) -> JsEngine {
        self.builder.build()
    }
}

/// Send-safe handle for emitting events into the engine from other threads.
pub struct EventHandle {
    event_channels: Arc<EventChannels>,
    logger: Logger,
}

impl EventHandle {
    pub fn emit(&self, event: &str, data: String) {
        if !self.event_channels.send(event, data, &self.logger) {
            panic!("emit: event \"{event}\" not registered with event_channel()");
        }
    }
}

pub struct JsEngine {
    setups: Vec<PluginFn>,
    userdata: Vec<UserdataFn>,
    event_channels: Arc<EventChannels>,
    logger: Logger,
    stack_size: Option<usize>,
}

impl JsEngine {
    pub fn builder() -> JsEngineBuilder {
        JsEngineBuilder {
            plugins: Vec::new(),
            userdata: Vec::new(),
            logger: None,
            event_channels: Vec::new(),
            stack_size: None,
        }
    }

    pub fn new() -> Self {
        Self::builder().build()
    }

    /// Returns a Send-safe handle for emitting events from other threads.
    pub fn event_handle(&self) -> EventHandle {
        EventHandle {
            event_channels: self.event_channels.clone(),
            logger: self.logger.clone(),
        }
    }

    /// Evaluate pre-compiled bytecode as a module and run the event loop.
    pub async fn eval(self, bytecode: Vec<u8>) {
        self.run(|ctx| {
            use rquickjs::{CatchResultExt, Module};
            let loaded = unsafe { Module::load(ctx.clone(), &bytecode) };
            match loaded {
                Ok(module) => match module.eval().map(|(_, promise)| promise).catch(&ctx) {
                    Ok(promise) => log_rejected(&ctx, promise.into_value()),
                    Err(e) => ctx.logger().error(&format!("module error: {e:?}")),
                },
                Err(e) => ctx.logger().error(&format!("bytecode load error: {e}")),
            }
        })
        .await;
    }

    /// Evaluate JS source as a module and run the event loop.
    #[cfg(feature = "compile")]
    pub async fn eval_source(self, code: &str) {
        let code = code.to_string();
        self.run(move |ctx| {
            use rquickjs::{CatchResultExt, Module};
            match Module::evaluate(ctx.clone(), "main", code).catch(&ctx) {
                Ok(promise) => log_rejected(&ctx, promise.into_value()),
                Err(e) => ctx.logger().error(&format!("module error: {e:?}")),
            }
        })
        .await;
    }

    async fn run<F>(self, task: F)
    where
        F: for<'js> FnOnce(Ctx<'js>) + Send,
    {
        let shutdown_hooks = ShutdownHooks::new();
        let logger = self.logger.clone();
        let (runtime, context, pending) = plugins::init_context(
            self.setups,
            self.userdata,
            self.logger,
            self.stack_size,
            shutdown_hooks.clone(),
        )
        .await;
        let event_channels = self.event_channels;

        context.with(|ctx| task(ctx)).await;

        loop {
            context
                .with(|ctx| {
                    crate::plugins::events::drain_and_dispatch(&ctx, &event_channels);
                })
                .await;

            tokio::select! {
                _ = event_channels.notified() => {}
                _ = pending.notified() => {}
                _ = runtime.idle() => {
                    if pending.is_idle() {
                        break;
                    }
                    tokio::task::yield_now().await;
                    tokio::time::sleep(std::time::Duration::from_micros(1000)).await;
                }
            }
        }

        shutdown_hooks.run(&logger);
    }
}

fn log_rejected<'js>(ctx: &Ctx<'js>, val: rquickjs::Value<'js>) {
    use rquickjs::promise::PromiseState;
    use rquickjs::Value;
    if let Some(promise) = val.as_promise() {
        if let PromiseState::Rejected = promise.state() {
            let err: Value = promise.result().unwrap().unwrap_or_else(|_| ctx.catch());
            if let Some(exc) = err.as_exception() {
                ctx.logger().error(&format!("{exc}"));
            } else {
                ctx.logger().error(&format!("{err:?}"));
            }
        }
    }
}
