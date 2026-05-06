use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::{ArrayBuffer, Ctx, Function, TypedArray, Value};

fn alloc<'js>(ctx: Ctx<'js>, size: usize) -> rquickjs::Result<Value<'js>> {
    let ab = ArrayBuffer::new_copy(ctx.clone(), vec![0u8; size])?;
    TypedArray::<u8>::from_arraybuffer(ab).map(|ta| ta.into_value())
}

fn throw_str(ctx: &Ctx<'_>, msg: &str) -> rquickjs::Error {
    ctx.throw(
        rquickjs::String::from_str(ctx.clone(), msg)
            .unwrap()
            .into(),
    )
}

pub struct MemoryModule;

impl ModuleDef for MemoryModule {
    fn declare<'js>(decl: &Declarations<'js>) -> rquickjs::Result<()> {
        decl.declare("alloc")?;
        decl.declare("free")?;
        decl.declare("memset")?;
        decl.declare("memset32")?;
        Ok(())
    }

    fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
        let alloc_fn = Function::new(ctx.clone(), alloc)?;

        let free_fn = Function::new(
            ctx.clone(),
            |_ctx: Ctx<'_>, data: TypedArray<'_, u8>| -> rquickjs::Result<()> {
                let mut ab = data.arraybuffer()?;
                ab.detach();
                Ok(())
            },
        )?;

        let memset_fn = Function::new(
            ctx.clone(),
            |ctx: Ctx<'_>,
             data: TypedArray<'_, u8>,
             offset: usize,
             length: usize,
             value: u8|
             -> rquickjs::Result<()> {
                let raw = data.as_raw().ok_or_else(|| throw_str(&ctx, "detached buffer"))?;
                if offset + length > raw.len {
                    return Err(throw_str(&ctx, "memset: offset + length out of bounds"));
                }
                let buf =
                    unsafe { std::slice::from_raw_parts_mut(raw.ptr.as_ptr().add(offset), length) };
                buf.fill(value);
                Ok(())
            },
        )?;

        let memset32_fn = Function::new(
            ctx.clone(),
            |ctx: Ctx<'_>,
             data: TypedArray<'_, u8>,
             offset: usize,
             length: usize,
             value: u32|
             -> rquickjs::Result<()> {
                let raw = data.as_raw().ok_or_else(|| throw_str(&ctx, "detached buffer"))?;
                if (offset + length) * 4 > raw.len {
                    return Err(throw_str(&ctx, "memset32: offset + length out of bounds (u32 units)"));
                }
                let buf = unsafe {
                    std::slice::from_raw_parts_mut(
                        (raw.ptr.as_ptr() as *mut u32).add(offset),
                        length,
                    )
                };
                buf.fill(value);
                Ok(())
            },
        )?;

        exports.export("alloc", alloc_fn)?;
        exports.export("free", free_fn)?;
        exports.export("memset", memset_fn)?;
        exports.export("memset32", memset32_fn)?;
        Ok(())
    }
}
