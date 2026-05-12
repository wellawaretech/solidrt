use crate::rendertree::{self, PlatformContext};
use crate::AlloyContext;
use crate::plugins;
use alloy::impellers::DisplayListBuilder;
use flux::rquickjs::{Ctx as QuickJsContext, Function};
use std::sync::Arc;

pub fn init(qtx: QuickJsContext<'_>, platform: Arc<PlatformContext>, atx: AlloyContext) {
  let draw_fn = Function::new(qtx.clone(), move |qtx: QuickJsContext<'_>| {
    let tree = qtx.userdata::<plugins::tree::SharedRenderTree>().unwrap();
    let mut builder = DisplayListBuilder::new(None);
    rendertree::composite::composite(&mut builder, &mut tree.0.borrow_mut(), &platform);
    if let Some(dl) = builder.build() {
      atx.submit(dl).expect("Failed to submit display list");
    }
  })
  .unwrap();

  let globals = qtx.globals();
  globals.set("draw", draw_fn).unwrap();
}
