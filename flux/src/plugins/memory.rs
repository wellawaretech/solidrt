use rquickjs::{Ctx, Function, TypedArray, Value};

fn alloc<'js>(ctx: Ctx<'js>, size: usize) -> rquickjs::Result<Value<'js>> {
    TypedArray::<u8>::new(ctx.clone(), vec![0u8; size]).map(|ta| ta.into_value())
}

fn throw_str(ctx: &Ctx<'_>, msg: &str) -> rquickjs::Error {
    ctx.throw(
        rquickjs::String::from_str(ctx.clone(), msg)
            .unwrap()
            .into(),
    )
}

pub fn init_memory(ctx: &Ctx<'_>) {
    let alloc_fn = Function::new(ctx.clone(), alloc).unwrap();

    let fill_fn = Function::new(
        ctx.clone(),
        |ctx: Ctx<'_>,
         data: TypedArray<'_, u8>,
         offset: usize,
         length: usize,
         value: u8|
         -> rquickjs::Result<()> {
            let raw = data.as_raw().ok_or_else(|| throw_str(&ctx, "detached buffer"))?;
            if offset + length > raw.len {
                return Err(throw_str(&ctx, "fill: offset + length out of bounds"));
            }
            let buf = unsafe { std::slice::from_raw_parts_mut(raw.ptr.as_ptr().add(offset), length) };
            buf.fill(value);
            Ok(())
        },
    )
    .unwrap();

    let fill32_fn = Function::new(
        ctx.clone(),
        |ctx: Ctx<'_>,
         data: TypedArray<'_, u8>,
         offset: usize,
         length: usize,
         value: u32|
         -> rquickjs::Result<()> {
            let raw = data.as_raw().ok_or_else(|| throw_str(&ctx, "detached buffer"))?;
            if offset + length * 4 > raw.len {
                return Err(throw_str(&ctx, "fill32: offset + length*4 out of bounds"));
            }
            let buf = unsafe { std::slice::from_raw_parts_mut(raw.ptr.as_ptr().add(offset), length * 4) };
            let pixel = value.to_le_bytes();
            for chunk in buf.chunks_exact_mut(4) {
                chunk.copy_from_slice(&pixel);
            }
            Ok(())
        },
    )
    .unwrap();

    let globals = ctx.globals();
    globals.set("alloc", alloc_fn).unwrap();
    globals.set("fill", fill_fn).unwrap();
    globals.set("fill32", fill32_fn).unwrap();
}
