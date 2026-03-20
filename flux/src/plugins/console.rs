use rquickjs::{Ctx, Function, Object, Value};

use crate::engine::Logger;

fn format_args<'js>(ctx: &Ctx<'js>, args: &[Value<'js>]) -> String {
    args.iter()
        .map(|v| format_value(ctx, v))
        .collect::<Vec<_>>()
        .join(" ")
}

fn format_value<'js>(ctx: &Ctx<'js>, val: &Value<'js>) -> String {
    if val.is_undefined() {
        "undefined".into()
    } else if val.is_null() {
        "null".into()
    } else if let Some(s) = val.as_string() {
        s.to_string().unwrap_or_default()
    } else if let Some(b) = val.as_bool() {
        b.to_string()
    } else {
        ctx.json_stringify(val.clone())
            .ok()
            .flatten()
            .and_then(|s| s.to_string().ok())
            .unwrap_or_else(|| "[object]".into())
    }
}

fn console_log<'js>(ctx: Ctx<'js>, args: rquickjs::function::Rest<Value<'js>>) {
    let msg = format_args(&ctx, &args.0);
    let logger = ctx.userdata::<Logger>().unwrap();
    logger.log(&msg);
}

fn console_warn<'js>(ctx: Ctx<'js>, args: rquickjs::function::Rest<Value<'js>>) {
    let msg = format_args(&ctx, &args.0);
    let logger = ctx.userdata::<Logger>().unwrap();
    logger.warn(&msg);
}

fn console_error<'js>(ctx: Ctx<'js>, args: rquickjs::function::Rest<Value<'js>>) {
    let msg = format_args(&ctx, &args.0);
    let logger = ctx.userdata::<Logger>().unwrap();
    logger.error(&msg);
}

pub(crate) fn init_console(ctx: &Ctx<'_>) {
    let globals = ctx.globals();
    let console = Object::new(ctx.clone()).unwrap();

    let log = Function::new(ctx.clone(), console_log).unwrap();
    let warn = Function::new(ctx.clone(), console_warn).unwrap();
    let error = Function::new(ctx.clone(), console_error).unwrap();

    console.set("log", log).unwrap();
    console.set("warn", warn).unwrap();
    console.set("error", error).unwrap();

    globals.set("console", console).unwrap();
}