use rquickjs::{Ctx, Function, TypedArray, Value};

fn alloc<'js>(ctx: Ctx<'js>, size: usize) -> rquickjs::Result<Value<'js>> {
    TypedArray::<u8>::new(ctx.clone(), vec![0u8; size]).map(|ta| ta.into_value())
}

pub fn init_memory(ctx: &Ctx<'_>) {
    let alloc_fn = Function::new(ctx.clone(), alloc).unwrap();
    ctx.globals().set("alloc", alloc_fn).unwrap();
}
