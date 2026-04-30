pub mod console;
pub mod events;
pub mod fetch;
pub mod io;
pub mod timer;
pub mod memory;

use rquickjs::loader::{BuiltinResolver, ModuleLoader};
use rquickjs::{AsyncContext, AsyncRuntime, Ctx};

use crate::engine::ShutdownHooks;
use crate::logger::Logger;
use crate::pending::PendingOps;

pub(crate) type PluginFn = Box<dyn for<'js> FnOnce(Ctx<'js>) + Send>;

pub(crate) async fn init_context(setups: Vec<PluginFn>, logger: Logger, stack_size: Option<usize>, shutdown_hooks: ShutdownHooks) -> (AsyncRuntime, AsyncContext, PendingOps) {
    let runtime = AsyncRuntime::new().expect("failed to create JS runtime");

    if let Some(limit) = stack_size {
        runtime.set_max_stack_size(limit).await;
    }

    let mut resolver = BuiltinResolver::default();
    let mut loader = ModuleLoader::default();

    resolver
        .add_module("qjs:memory")
        .add_module("qjs:io");
    loader
        .add_module("qjs:memory", memory::MemoryModule)
        .add_module("qjs:io", io::IoModule);

    runtime.set_loader(resolver, loader).await;

    let context = AsyncContext::full(&runtime)
        .await
        .expect("failed to create JS context");

    let pending = PendingOps::new();

    context
        .with(|ctx| {
            ctx.store_userdata(pending.clone()).unwrap();
            ctx.store_userdata(logger).unwrap();
            ctx.store_userdata(shutdown_hooks).unwrap();
            timer::init_timers(&ctx);
            io::init_io(&ctx);
            fetch::init_fetch(&ctx);
            console::init_console(&ctx);

            events::init_events(&ctx);
            for setup in setups {
                setup(ctx.clone());
            }
        })
        .await;

    (runtime, context, pending)
}
